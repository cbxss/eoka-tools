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

type rpcRequest struct {
	ID     int64  `json:"id"`
	Method string `json:"method"`
	Params any    `json:"params"`
}

type rpcError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

type rpcResponse struct {
	ID     int64           `json:"id"`
	Result json.RawMessage `json:"result"`
	Error  *rpcError       `json:"error"`
}

type transport struct {
	w       io.WriteCloser
	writeMu sync.Mutex

	r *bufio.Reader

	mu       sync.Mutex
	pending  map[int64]chan *rpcResponse
	closeErr error

	nextID atomic.Int64
}

func newTransport(w io.WriteCloser, r io.Reader) *transport {
	return &transport{
		w:       w,
		r:       bufio.NewReaderSize(r, 256*1024),
		pending: make(map[int64]chan *rpcResponse),
	}
}

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
}

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

func (t *transport) closed() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.closeErr
}

func (t *transport) shutdown() {
	_ = t.w.Close()
}

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
