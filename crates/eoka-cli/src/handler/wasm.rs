//! `wasm` subcommand group: read/write/find/info against WebAssembly linear memory.

use serde_json::Value;

use super::Handler;
use crate::protocol::Response;

impl Handler {
    pub(super) async fn cmd_wasm_info(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let js = include_str!("../js/wasm_info.js");
        let result: String = tab
            .page
            .evaluate_sync(js)
            .await
            .map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
        Ok(Response::ok(parsed))
    }

    pub(super) async fn cmd_wasm_read(&mut self, args: &Value) -> Result<Response, String> {
        let addr = parse_addr(self.arg_str(args, "addr")?)?;
        let len = args["len"].as_u64().ok_or("Missing 'len'")? as usize;
        let memory = args["memory"].as_str();

        let tab = self.require_tab()?;
        let mem_expr = wasm_memory_expr(memory);

        let chunk_size = 65536;
        let mut hex_output = String::new();
        let mut offset = 0;

        while offset < len {
            let chunk_len = (len - offset).min(chunk_size);
            let js = format!(
                r#"(() => {{
                    const mem = {mem};
                    if (!mem || !mem.buffer) return null;
                    const buf = new Uint8Array(mem.buffer, {addr} + {offset}, {chunk_len});
                    return Array.from(buf).map(b => b.toString(16).padStart(2, '0')).join('');
                }})()"#,
                mem = mem_expr,
                addr = addr,
                offset = offset,
                chunk_len = chunk_len,
            );
            let chunk: String = tab
                .page
                .evaluate_sync(&js)
                .await
                .map_err(|e| e.to_string())?;
            if chunk == "null" || chunk.is_empty() {
                return Err(format!(
                    "WASM memory not found or address out of bounds (tried {})",
                    mem_expr
                ));
            }
            hex_output.push_str(&chunk);
            offset += chunk_len;
        }

        let mut formatted = String::new();
        let bytes_per_line = 16;
        for (i, chunk) in hex_output.as_bytes().chunks(bytes_per_line * 2).enumerate() {
            let line_addr = addr
                .checked_add(i * bytes_per_line)
                .ok_or("Address overflow while formatting WASM memory dump")?;
            use std::fmt::Write;
            let _ = write!(formatted, "{:08x}  ", line_addr);
            for (j, pair) in chunk.chunks(2).enumerate() {
                if j == 8 {
                    formatted.push(' ');
                }
                formatted.push(pair[0] as char);
                formatted.push(pair[1] as char);
                formatted.push(' ');
            }
            formatted.push('\n');
        }

        Ok(Response::ok_text(formatted))
    }

    pub(super) async fn cmd_wasm_write(&mut self, args: &Value) -> Result<Response, String> {
        let addr = parse_addr(self.arg_str(args, "addr")?)?;
        let hex = self.arg_str(args, "hex")?;
        let memory = args["memory"].as_str();

        let hex_clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if !hex_clean.len().is_multiple_of(2) {
            return Err("Hex string must have even length".into());
        }

        let mem_expr = wasm_memory_expr(memory);
        let js = format!(
            r#"(() => {{
                const mem = {mem};
                if (!mem || !mem.buffer) return 'error: WASM memory not found';
                const bytes = new Uint8Array([{byte_array}]);
                new Uint8Array(mem.buffer).set(bytes, {addr});
                return 'ok';
            }})()"#,
            mem = mem_expr,
            addr = addr,
            byte_array = hex_to_byte_array(&hex_clean),
        );

        let tab = self.require_tab()?;
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        if result.starts_with("error:") {
            return Err(result);
        }

        Ok(Response::ok_text(format!(
            "Wrote {} bytes at 0x{:x}",
            hex_clean.len() / 2,
            addr
        )))
    }

    pub(super) async fn cmd_wasm_find(&mut self, args: &Value) -> Result<Response, String> {
        let pattern = self.arg_str(args, "pattern")?;
        let memory = args["memory"].as_str();
        let max = args["max"].as_u64().unwrap_or(20) as usize;
        let start = args["start"]
            .as_str()
            .map(parse_addr)
            .transpose()?
            .unwrap_or(0);
        let end = args["end"].as_str().map(parse_addr).transpose()?;

        let pattern_clean: String = pattern.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if !pattern_clean.len().is_multiple_of(2) || pattern_clean.is_empty() {
            return Err("Pattern must be non-empty hex with even length".into());
        }

        let mem_expr = wasm_memory_expr(memory);
        let end_expr = match end {
            Some(e) => format!("{}", e),
            None => "buf.length".to_string(),
        };

        let js = format!(
            r#"(() => {{
                const mem = {mem};
                if (!mem || !mem.buffer) return JSON.stringify({{error: 'WASM memory not found'}});
                const buf = new Uint8Array(mem.buffer);
                const pattern = [{byte_array}];
                const results = [];
                const end = Math.min({end_expr}, buf.length);
                for (let i = {start}; i <= end - pattern.length; i++) {{
                    let match = true;
                    for (let j = 0; j < pattern.length; j++) {{
                        if (buf[i + j] !== pattern[j]) {{ match = false; break; }}
                    }}
                    if (match) {{
                        results.push(i);
                        if (results.length >= {max}) break;
                    }}
                }}
                return JSON.stringify({{matches: results, searched: end - {start}}});
            }})()"#,
            mem = mem_expr,
            byte_array = hex_to_byte_array(&pattern_clean),
            start = start,
            end_expr = end_expr,
            max = max,
        );

        let tab = self.require_tab()?;
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Null);

        if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }

        let matches = parsed.get("matches").and_then(|v| v.as_array());
        let searched = parsed.get("searched").and_then(|v| v.as_u64()).unwrap_or(0);

        match matches {
            Some(addrs) if !addrs.is_empty() => {
                let mut out = format!(
                    "Found {} matches (searched {} bytes):\n",
                    addrs.len(),
                    searched
                );
                for a in addrs {
                    if let Some(addr) = a.as_u64() {
                        use std::fmt::Write;
                        let _ = writeln!(out, "  0x{:08x}", addr);
                    }
                }
                Ok(Response::ok_text(out))
            }
            _ => Ok(Response::ok_text(format!(
                "No matches found (searched {} bytes)",
                searched
            ))),
        }
    }
}

fn parse_addr(s: &str) -> Result<usize, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        usize::from_str_radix(hex, 16).map_err(|e| format!("Invalid hex address '{}': {}", s, e))
    } else {
        s.parse()
            .map_err(|e| format!("Invalid address '{}': {}", s, e))
    }
}

fn wasm_memory_expr(memory: Option<&str>) -> String {
    match memory {
        Some(expr) => expr.to_string(),
        None => {
            r#"(window.__ft?.mem || window.Module?.wasmMemory || window.Module?.HEAPU8?.buffer && {buffer: window.Module.HEAPU8.buffer} || (() => { for (const k of Object.keys(window)) { try { if (window[k] instanceof WebAssembly.Memory) return window[k]; } catch(e){} } return null; })())"#.to_string()
        }
    }
}

fn hex_to_byte_array(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| format!("0x{}{}", pair[0] as char, pair[1] as char))
        .collect::<Vec<_>>()
        .join(",")
}
