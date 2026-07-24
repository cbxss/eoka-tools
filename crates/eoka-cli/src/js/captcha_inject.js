((token, kind, callbackExpression) => {
    const requested = (kind || 'auto').toLowerCase();
    const updated = [];
    const created = [];
    const callbacks = [];
    const errors = [];

    function wants(name) {
        return requested === 'auto' || requested === name;
    }

    function setValue(el, value) {
        el.value = value;
        el.textContent = value;
        for (const eventName of ['input', 'change']) {
            el.dispatchEvent(new Event(eventName, { bubbles: true }));
        }
    }

    function updateSelector(selector) {
        for (const el of document.querySelectorAll(selector)) {
            setValue(el, token);
            updated.push(selector);
        }
    }

    function ensureTextarea(name) {
        if (document.querySelector(`[name="${name}"]`)) return;
        const textarea = document.createElement('textarea');
        textarea.name = name;
        textarea.id = name;
        textarea.style.display = 'none';
        document.body.appendChild(textarea);
        setValue(textarea, token);
        created.push(name);
    }

    if (wants('recaptcha')) {
        updateSelector('textarea[name="g-recaptcha-response"], #g-recaptcha-response');
        ensureTextarea('g-recaptcha-response');
    }
    if (wants('hcaptcha')) {
        updateSelector('textarea[name="h-captcha-response"], #h-captcha-response');
        ensureTextarea('h-captcha-response');
    }
    if (wants('turnstile')) {
        updateSelector('input[name="cf-turnstile-response"], textarea[name="cf-turnstile-response"], #cf-turnstile-response');
        ensureTextarea('cf-turnstile-response');
    }

    function call(fn, path) {
        try {
            fn(token);
            callbacks.push(path);
        } catch (error) {
            errors.push(`${path}: ${error && error.message ? error.message : String(error)}`);
        }
    }

    if (callbackExpression) {
        try {
            const fn = Function(`return (${callbackExpression})`)();
            if (typeof fn === 'function') {
                call(fn, callbackExpression);
            } else {
                errors.push(`${callbackExpression}: not a function`);
            }
        } catch (error) {
            errors.push(`${callbackExpression}: ${error && error.message ? error.message : String(error)}`);
        }
    }

    function shouldCallCallbackKey(key) {
        const normalized = String(key || '').toLowerCase();
        return normalized.includes('callback') &&
            !normalized.includes('expired') &&
            !normalized.includes('error');
    }

    function walkCallbacks(root, path, seen, depth) {
        if (!root || depth > 6) return;
        if (typeof root !== 'object' && typeof root !== 'function') return;
        if (seen.has(root)) return;
        seen.add(root);

        for (const key of Object.keys(root)) {
            let value;
            try {
                value = root[key];
            } catch (_error) {
                continue;
            }

            const childPath = `${path}.${key}`;
            if (typeof value === 'function' && shouldCallCallbackKey(key)) {
                call(value, childPath);
            } else if (value && (typeof value === 'object' || typeof value === 'function')) {
                walkCallbacks(value, childPath, seen, depth + 1);
            }
        }
    }

    if (wants('recaptcha') && window.___grecaptcha_cfg?.clients) {
        walkCallbacks(window.___grecaptcha_cfg.clients, '___grecaptcha_cfg.clients', new WeakSet(), 0);
    }
    if (wants('hcaptcha') && window.___hcaptcha_cfg?.clients) {
        walkCallbacks(window.___hcaptcha_cfg.clients, '___hcaptcha_cfg.clients', new WeakSet(), 0);
    }

    return JSON.stringify({
        kind: requested,
        updated_count: updated.length,
        created,
        callbacks,
        errors,
    });
})
