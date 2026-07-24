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

    function textCandidateScore(el, query) {
        const tag = el.tagName.toLowerCase();
        const role = lc(el.getAttribute('role'));
        const textValue = lc(text(el));
        const exact = textValue === query ? 1 : 0;
        let control = 0;

        if (tag === 'button') control = 100;
        else if (role === 'button') control = 90;
        else if (tag === 'input' && ['button', 'submit', 'reset'].includes(lc(el.type))) control = 85;
        else if (tag === 'a') control = 70;
        else if (['select', 'textarea', 'input'].includes(tag)) control = 60;
        else if (el.hasAttribute('onclick')) control = 25;
        else if (el.hasAttribute('tabindex')) control = 20;

        let depth = 0;
        for (let n = el; n && n.nodeType === 1; n = n.parentElement) depth++;
        const r = el.getBoundingClientRect();
        const area = r.width * r.height;

        return { exact, control, depth, area };
    }

    function bestTextMatch(elements, query) {
        return elements
            .map((el, index) => ({ el, index, score: textCandidateScore(el, query) }))
            .sort((a, b) =>
                (b.score.control - a.score.control) ||
                (b.score.exact - a.score.exact) ||
                (b.score.depth - a.score.depth) ||
                (a.score.area - b.score.area) ||
                (a.index - b.index)
            )[0]?.el || null;
    }

    let el = null;
    switch (type) {
        case 'text':
            el = bestTextMatch(
                interactive().filter(e => lc(text(e)).includes(valLc)),
                valLc
            );
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
