# eoka-protocol

Protocol types and tool catalog metadata for Eoka browser automation.

This crate is the source of truth for daemon request wire commands, stable tool
paths, operation metadata, input schemas, and manifest projection. Downstream
surfaces such as the CLI manifest, Tack adapter, SDK, and MCP integration use
these definitions to avoid drift.

