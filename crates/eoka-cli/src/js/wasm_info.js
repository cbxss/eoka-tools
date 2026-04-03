(function() {
  var results = [];

  // Check well-known globals
  var knownPaths = [
    ['window.__ft', 'mem'],
    ['window.Module', 'HEAPU8'],
    ['window.Module', 'wasmMemory'],
    ['window._emscripten_memory', null],
  ];

  for (var i = 0; i < knownPaths.length; i++) {
    try {
      var obj = (new Function('return ' + knownPaths[i][0]))();
      if (!obj) continue;
      var mem = knownPaths[i][1] ? obj[knownPaths[i][1]] : obj;
      if (mem && mem.buffer && mem.buffer instanceof ArrayBuffer) {
        results.push({
          path: knownPaths[i][0] + (knownPaths[i][1] ? '.' + knownPaths[i][1] : ''),
          type: 'WebAssembly.Memory',
          size: mem.buffer.byteLength,
          pages: mem.buffer.byteLength / 65536
        });
      }
    } catch(e) {}
  }

  // Scan window for WebAssembly.Memory instances (shallow)
  try {
    var keys = Object.keys(window);
    for (var k = 0; k < keys.length; k++) {
      try {
        var val = window[keys[k]];
        if (val instanceof WebAssembly.Memory) {
          results.push({
            path: 'window.' + keys[k],
            type: 'WebAssembly.Memory',
            size: val.buffer.byteLength,
            pages: val.buffer.byteLength / 65536
          });
        }
        if (val && val.memory instanceof WebAssembly.Memory) {
          results.push({
            path: 'window.' + keys[k] + '.memory',
            type: 'WebAssembly.Memory',
            size: val.memory.buffer.byteLength,
            pages: val.memory.buffer.byteLength / 65536
          });
        }
      } catch(e) {}
    }
  } catch(e) {}

  return JSON.stringify(results);
})()
