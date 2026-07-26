package eoka

import (
	"errors"
	"fmt"
)

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

type Error struct {
	Code    string
	Message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("eoka: %s: %s", e.Code, e.Message)
}

func HasCode(err error, code string) bool {
	var e *Error
	if errors.As(err, &e) {
		return e.Code == code
	}
	return false
}
