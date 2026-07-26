package eoka

type SessionCookie struct {
	Name     string   `json:"name"`
	Value    string   `json:"value"`
	Domain   string   `json:"domain"`
	Path     string   `json:"path"`
	Secure   bool     `json:"secure"`
	HTTPOnly bool     `json:"http_only"`
	SameSite *string  `json:"same_site"`
	Expires  *float64 `json:"expires"`
}

type BrowserState struct {
	Cookies        []SessionCookie   `json:"cookies"`
	LocalStorage   map[string]string `json:"localStorage"`
	SessionStorage map[string]string `json:"sessionStorage"`
	UserAgent      string            `json:"userAgent"`
	URL            string            `json:"url"`
}
