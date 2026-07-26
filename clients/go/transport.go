package eoka

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sync"
	"sync/atomic"
)

// rpcRequest is the wire shape of a request sent to eoka-server.
type rpcRequest struct {
	ID     int64  `json:"id"`
	Method string `json:"method"`
	Params any    `json:"params"`
}

// rpcError is the wire shape of a response's "error" field.
type rpcError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// rpcResponse is the wire shape of a response received from eoka-server.
// Exactly one of Result/Error is populated per PROTOCOL.md.
type rpcResponse struct {
	ID     int64           `json:"id"`
	Result json.RawMessage `json:"result"`
	Error  *rpcError       `json:"error"`
}

// transport implements the NDJSON request/response correlation layer over
// an eoka-server child process's stdin/stdout (or, in tests, a pair of
// in-memory pipes standing in for them).
type transport struct {
	w       io.WriteCloser
	writeMu sync.Mutex

	r *bufio.Reader

	mu       sync.Mutex
	pending  map[int64]chan *rpcResponse
	closeErr error

	nextID atomic.Int64
}

// newTransport wires a transport to the given request writer and response
// reader. bufio.Reader.ReadString has no fixed max token size (unlike
// bufio.Scanner's default 64KB limit), so large payloads such as
// page.content HTML or page.screenshot base64 PNGs are never truncated
// regardless of buffer size; the size below is just the per-read chunk,
// sized up from bufio's 4KB default since those payloads are common here.
func newTransport(w io.WriteCloser, r io.Reader) *transport {
	return &transport{
		w:       w,
		r:       bufio.NewReaderSize(r, 256*1024),
		pending: make(map[int64]chan *rpcResponse),
	}
}

// readLoop reads response lines until the underlying reader errors (EOF
// when the server process exits, or a pipe error). It must run in its own
// goroutine for the lifetime of the transport.
func (t *transport) readLoop() {
	for {
		line, err := t.r.ReadString('\n')
		if len(line) > 0 {
			t.handleLine(line)
		}
		if err != nil {
			t.fail(fmt.Errorf("eoka: reading from eoka-server: %w", err))
			return
		}
	}
}

func (t *transport) handleLine(line string) {
	var resp rpcResponse
	if err := json.Unmarshal([]byte(line), &resp); err != nil {
		t.fail(fmt.Errorf("eoka: malformed response from eoka-server: %w", err))
		return
	}

	t.mu.Lock()
	ch, ok := t.pending[resp.ID]
	if ok {
		delete(t.pending, resp.ID)
	}
	t.mu.Unlock()

	if ok {
		ch <- &resp
	}
	// A response for an unknown id (already discarded after a context
	// timeout, or a server bug) is silently ignored.
}

// fail marks the transport permanently closed with err, releasing every
// in-flight call. Only the first call has an effect.
func (t *transport) fail(err error) {
	t.mu.Lock()
	if t.closeErr != nil {
		t.mu.Unlock()
		return
	}
	t.closeErr = err
	pending := make([]chan *rpcResponse, 0, len(t.pending))
	for id, ch := range t.pending {
		pending = append(pending, ch)
		delete(t.pending, id)
	}
	t.mu.Unlock()

	for _, ch := range pending {
		close(ch)
	}
}

// closed reports the transport's terminal error, if any.
func (t *transport) closed() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.closeErr
}

// shutdown closes the write side of the transport (the child process's
// stdin), signalling it to exit per PROTOCOL.md's process lifecycle rules.
func (t *transport) shutdown() {
	_ = t.w.Close()
}

// call sends a request and blocks until a matching response arrives, ctx is
// done, or the transport fails. On success, result (if non-nil) is decoded
// from the response's "result" field. On a protocol error response, call
// returns a *Error. On ctx cancellation, call returns ctx.Err() and
// discards the in-flight request's response if it arrives later.
func (t *transport) call(ctx context.Context, method string, params any, result any) error {
	if err := t.closed(); err != nil {
		return err
	}

	id := t.nextID.Add(1)
	ch := make(chan *rpcResponse, 1)

	t.mu.Lock()
	if t.closeErr != nil {
		err := t.closeErr
		t.mu.Unlock()
		return err
	}
	t.pending[id] = ch
	t.mu.Unlock()

	data, err := json.Marshal(rpcRequest{ID: id, Method: method, Params: params})
	if err != nil {
		t.mu.Lock()
		delete(t.pending, id)
		t.mu.Unlock()
		return fmt.Errorf("eoka: marshaling request: %w", err)
	}
	data = append(data, '\n')

	t.writeMu.Lock()
	_, werr := t.w.Write(data)
	t.writeMu.Unlock()
	if werr != nil {
		t.mu.Lock()
		delete(t.pending, id)
		t.mu.Unlock()
		return fmt.Errorf("eoka: writing request to eoka-server: %w", werr)
	}

	select {
	case resp, ok := <-ch:
		if !ok {
			if err := t.closed(); err != nil {
				return err
			}
			return errors.New("eoka: transport closed")
		}
		if resp.Error != nil {
			return &Error{Code: resp.Error.Code, Message: resp.Error.Message}
		}
		if result != nil && len(resp.Result) > 0 {
			if err := json.Unmarshal(resp.Result, result); err != nil {
				return fmt.Errorf("eoka: decoding result: %w", err)
			}
		}
		return nil
	case <-ctx.Done():
		t.mu.Lock()
		delete(t.pending, id)
		t.mu.Unlock()
		return ctx.Err()
	}
}
