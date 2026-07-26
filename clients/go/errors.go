package eoka

import (
	"errors"
	"fmt"
)

// Error codes returned by eoka-server, mapped from eoka::Error. See
// PROTOCOL.md's "Error codes" table for the authoritative meaning of each.
const (
	ErrCodeElementNotFound   = "ElementNotFound"
	ErrCodeElementNotVisible = "ElementNotVisible"
	ErrCodeTimeout           = "Timeout"
	ErrCodeRetryExhausted    = "RetryExhausted"
	ErrCodeCdp               = "Cdp"
	ErrCodeInvalidPage       = "InvalidPage"
	ErrCodeInvalidParams     = "InvalidParams"
	ErrCodeUnknownMethod     = "UnknownMethod"
	ErrCodeInternal          = "Internal"
)

// Error is a typed protocol error returned by eoka-server in a response's
// "error" field. Callers should branch on Code, never on Message (which is
// a human-readable string for logs/debugging and is not stable API).
//
// Transport-level failures (the eoka-server process died, its pipes closed,
// a response failed to decode as JSON) are reported as plain Go errors, not
// as *Error, since they are not protocol error codes from the server.
type Error struct {
	Code    string
	Message string
}

// Error implements the error interface.
func (e *Error) Error() string {
	return fmt.Sprintf("eoka: %s: %s", e.Code, e.Message)
}

// HasCode reports whether err is an *Error with the given Code. Compare
// against the ErrCode* constants, e.g. HasCode(err, eoka.ErrCodeElementNotFound).
func HasCode(err error, code string) bool {
	var e *Error
	if errors.As(err, &e) {
		return e.Code == code
	}
	return false
}
