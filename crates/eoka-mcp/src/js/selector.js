function(el) {
    if (!el) return null;
    if (el.id) return '#' + CSS.escape(el.id);
    const parts = [];
    let cur = el;
    while (cur && cur !== document.documentElement) {
        let sel = cur.tagName.toLowerCase();
        if (cur.id) { parts.unshift('#' + CSS.escape(cur.id)); break; }
        const parent = cur.parentElement;
        if (parent) {
            const siblings = Array.from(parent.children).filter(c => c.tagName === cur.tagName);
            if (siblings.length > 1) {
                sel += ':nth-of-type(' + (siblings.indexOf(cur) + 1) + ')';
            }
        }
        parts.unshift(sel);
        cur = parent;
    }
    return parts.join(' > ');
}
