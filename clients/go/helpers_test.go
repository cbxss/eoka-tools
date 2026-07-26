package eoka

import (
	"bufio"
	"encoding/json"
	"io"
	"testing"
)

type fakeHandler func(id int64, method string, params json.RawMessage) (result any, rpcErr *rpcError)

func newFakeTransport(t *testing.T, handle fakeHandler) *transport {
	t.Helper()

	reqR, reqW := io.Pipe()
	respR, respW := io.Pipe()

	tr := newTransport(reqW, respR)
	go tr.readLoop()
	go runFakeServer(reqR, respW, handle)

	t.Cleanup(func() {
		_ = reqW.Close()
		_ = respW.Close()
	})

	return tr
}

func newFakeBrowser(t *testing.T, handle fakeHandler) *Browser {
	t.Helper()
	return &Browser{t: newFakeTransport(t, handle)}
}

func runFakeServer(r io.Reader, w io.Writer, handle fakeHandler) {
	br := bufio.NewReaderSize(r, 64*1024)
	for {
		line, readErr := br.ReadString('\n')
		if len(line) > 0 {
			var req struct {
				ID     int64           `json:"id"`
				Method string          `json:"method"`
				Params json.RawMessage `json:"params"`
			}
			if jsonErr := json.Unmarshal([]byte(line), &req); jsonErr == nil {
				result, rpcErr := handle(req.ID, req.Method, req.Params)

				resp := rpcResponse{ID: req.ID}
				if rpcErr != nil {
					resp.Error = rpcErr
				} else if result == nil {
					resp.Result = json.RawMessage(`{}`)
				} else if data, err := json.Marshal(result); err == nil {
					resp.Result = data
				}

				data, err := json.Marshal(resp)
				if err != nil {
					return
				}
				data = append(data, '\n')
				if _, werr := w.Write(data); werr != nil {
					return
				}
			}
		}
		if readErr != nil {
			return
		}
	}
}
