//! Save-state serialization for rewind + netplay. Implemented at milestone M5;
//! the ring-buffer rewind clones only mutable machine state (never the ROM) so a
//! frame snapshot stays cheap enough to take every single frame.

// Placeholder: the deterministic snapshot/restore lands with the debugger + rewind work.
