((type, value) => {
    const lc = s => (s || '').toLowerCase().trim();
    const valLc = lc(value);

    function selector(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        const path = [];
        let n = el;
        while (n && n.nodeType === 1) {
            let s = n.tagName.toLowerCase();
            if (n.id) { path.unshift('#' + CSS.escape(n.id)); break; }
            const p = n.parentElement;
            if (p) {
                const sibs = [...p.children].filter(c => c.tagName === n.tagName);
                if (sibs.length > 1) s += ':nth-of-type(' + (sibs.indexOf(n) + 1) + ')';
            }
            path.unshift(s);
            n = p;
        }
        return path.join(' > ');
    }

    function text(el) {
        return el.innerText?.trim() || el.value || el.getAttribute('aria-label') || el.title || el.placeholder || '';
    }

    function interactive() {
        return [...document.querySelectorAll('a,button,input,select,textarea,[role="button"],[onclick],[tabindex]')]
            .filter(el => {
                const r = el.getBoundingClientRect();
                const s = getComputedStyle(el);
                return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none';
            });
    }

    let el = null;
    switch (type) {
        case 'text':
            el = interactive().find(e => lc(text(e)).includes(valLc));
            break;
        case 'placeholder':
            el = document.querySelector(`input[placeholder*="${value}" i],textarea[placeholder*="${value}" i]`)
                || interactive().find(e => lc(e.placeholder).includes(valLc));
            break;
        case 'role':
            el = document.querySelector(valLc) || document.querySelector(`[role="${value}"]`)
                || interactive().find(e => e.tagName.toLowerCase() === valLc || e.getAttribute('role') === value);
            break;
        case 'css':
            el = document.querySelector(value);
            break;
        case 'id':
            el = document.getElementById(value);
            break;
    }

    if (!el) return { found: false, error: `${type}:${value} not found`, selector: '', tag: '', text: '', bbox: {x:0,y:0,width:0,height:0} };

    const r = el.getBoundingClientRect();
    return { found: true, selector: selector(el), tag: el.tagName.toLowerCase(), text: text(el).slice(0, 50), bbox: {x:r.x,y:r.y,width:r.width,height:r.height} };
})
