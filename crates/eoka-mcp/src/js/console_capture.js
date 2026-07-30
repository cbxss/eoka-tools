(function() {
    if (window.__eoka_console) return;

    var MAX_CONSOLE = 200;
    var MAX_ERRORS = 100;

    window.__eoka_console = [];
    window.__eoka_errors = [];

    function serialize(arg) {
        if (arg === undefined) return 'undefined';
        if (arg === null) return 'null';
        if (arg instanceof Error) return arg.stack || arg.message || String(arg);
        try { return JSON.stringify(arg); } catch(_) { return String(arg); }
    }

    function pushCircular(arr, entry, max) {
        arr.push(entry);
        if (arr.length > max) arr.splice(0, arr.length - max);
    }

    var orig = {};
    ['log','warn','error','info','debug'].forEach(function(level) {
        orig[level] = console[level];
        console[level] = function() {
            var args = Array.prototype.slice.call(arguments);
            pushCircular(window.__eoka_console, {
                level: level,
                text: args.map(serialize).join(' '),
                ts: Date.now()
            }, MAX_CONSOLE);
            orig[level].apply(console, arguments);
        };
    });

    window.addEventListener('error', function(e) {
        pushCircular(window.__eoka_errors, {
            message: e.message || String(e),
            source: e.filename || '',
            line: e.lineno || 0,
            col: e.colno || 0,
            stack: e.error && e.error.stack ? e.error.stack : '',
            ts: Date.now()
        }, MAX_ERRORS);
    });

    window.addEventListener('unhandledrejection', function(e) {
        var reason = e.reason;
        pushCircular(window.__eoka_errors, {
            message: reason instanceof Error ? reason.message : String(reason),
            source: '',
            line: 0,
            col: 0,
            stack: reason instanceof Error && reason.stack ? reason.stack : '',
            ts: Date.now()
        }, MAX_ERRORS);
    });
})();
