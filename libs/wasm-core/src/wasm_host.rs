//! WASM Host Runtime — Load and execute WASM modules via wasmtime
//!
//! This module provides the host-side integration for executing the
//! `wasm_core.wasm` module. It wraps `wasmtime` to load, instantiate,
//! and invoke the WASM FFI functions defined in `wasm_bindings.rs`.
//!
//! # Responsibilities
//!
//! - Loading `.wasm` bytes and compiling them
//! - Managing WASM linear memory (alloc / dealloc)
//! - Invoking `wasm_margin_preview` via the FFI protocol
//! - Converting between host strings and WASM memory
//!
//! # Non-responsibilities
//!
//! - Business logic (handled by `margin.rs`)
//! - Validation (handled by `wasm_adapter.rs`)
//! - Fallback decisions (handled by `wasm_adapter.rs`)
//! - State management (WASM is stateless per call)
//!
//! # FFI Protocol (matches wasm_bindings.rs)
//!
//! ```text
//! Host → WASM:
//!   1. wasm_alloc(len) → ptr
//!   2. Write JSON bytes to ptr in WASM memory
//!   3. wasm_margin_preview(ptr, len) → result_ptr
//!   4. Read [4 bytes: u32 LE length][N bytes: JSON] from result_ptr
//!   5. wasm_dealloc(ptr, len)       // cleanup input
//!   6. wasm_dealloc(result_ptr, 4+N) // cleanup output
//! ```
//!
//! # Safety
//!
//! All WASM memory access goes through wasmtime's safe API.
//! The WASM module runs in a sandboxed linear memory with no access to
//! host filesystem, network, or any WASI imports.

use std::path::Path;

use wasmtime::{Engine, Linker, Module, Store};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during WASM host operations.
#[derive(Debug)]
pub enum WasmHostError {
    /// Failed to create the wasmtime engine
    EngineCreation(String),
    /// Failed to compile the WASM module
    ModuleCompilation(String),
    /// Failed to instantiate the WASM module
    Instantiation(String),
    /// Required export function not found in the WASM module
    MissingExport(String),
    /// WASM memory allocation failed
    MemoryAllocation(String),
    /// Failed to read from or write to WASM linear memory
    MemoryAccess(String),
    /// WASM function execution failed
    Execution(String),
    /// Result from WASM is not valid UTF-8
    InvalidUtf8,
    /// I/O error loading WASM file
    IoError(String),
}

impl std::fmt::Display for WasmHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmHostError::EngineCreation(e) => write!(f, "Engine creation failed: {e}"),
            WasmHostError::ModuleCompilation(e) => write!(f, "Module compilation failed: {e}"),
            WasmHostError::Instantiation(e) => write!(f, "Module instantiation failed: {e}"),
            WasmHostError::MissingExport(name) => {
                write!(f, "Missing required WASM export: {name}")
            }
            WasmHostError::MemoryAllocation(e) => write!(f, "WASM memory allocation failed: {e}"),
            WasmHostError::MemoryAccess(e) => write!(f, "WASM memory access failed: {e}"),
            WasmHostError::Execution(e) => write!(f, "WASM execution failed: {e}"),
            WasmHostError::InvalidUtf8 => write!(f, "WASM result is not valid UTF-8"),
            WasmHostError::IoError(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for WasmHostError {}

// ---------------------------------------------------------------------------
// WASM Runtime
// ---------------------------------------------------------------------------

/// Host-side WASM runtime for executing margin preview computations.
///
/// Holds the compiled WASM module and engine. Each `margin_preview` call
/// creates a fresh store + instance, ensuring no state leaks between calls
/// (stateless, deterministic).
///
/// # Thread Safety
///
/// `WasmRuntime` is `Send + Sync`. The `Engine` and `Module` are both
/// thread-safe. Each call creates its own `Store` and `Instance`.
pub struct WasmRuntime {
    engine: Engine,
    module: Module,
}

impl WasmRuntime {
    /// Create a new WASM runtime from raw `.wasm` bytes.
    ///
    /// Compiles the module eagerly. Returns an error if compilation fails
    /// (e.g., invalid WASM, unsupported features).
    pub fn new(wasm_bytes: &[u8]) -> Result<Self, WasmHostError> {
        let engine = Engine::default();

        let module = Module::new(&engine, wasm_bytes)
            .map_err(|e| WasmHostError::ModuleCompilation(e.to_string()))?;

        Ok(Self { engine, module })
    }

    /// Create a new WASM runtime by loading a `.wasm` file from disk.
    pub fn from_file(path: &Path) -> Result<Self, WasmHostError> {
        let wasm_bytes =
            std::fs::read(path).map_err(|e| WasmHostError::IoError(e.to_string()))?;
        Self::new(&wasm_bytes)
    }

    /// Execute a margin preview computation through the WASM module.
    ///
    /// Follows the FFI protocol defined in `wasm_bindings.rs`:
    /// 1. Allocate input buffer in WASM memory
    /// 2. Write JSON input bytes
    /// 3. Call `wasm_margin_preview`
    /// 4. Read result (length-prefixed JSON)
    /// 5. Deallocate both buffers
    ///
    /// # Determinism
    ///
    /// Each call creates a fresh `Store` and `Instance`, so there is no
    /// hidden state between calls. Given identical input, the output is
    /// always identical.
    pub fn margin_preview(&self, input_json: &str) -> Result<String, WasmHostError> {
        // Fresh store per call — no state leakage
        let mut store = Store::new(&self.engine, ());

        // Instantiate with empty linker (no WASI, no imports)
        let linker = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| WasmHostError::Instantiation(e.to_string()))?;

        // Resolve required exports
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| WasmHostError::MissingExport("memory".to_owned()))?;

        let wasm_alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "wasm_alloc")
            .map_err(|e| WasmHostError::MissingExport(format!("wasm_alloc: {e}")))?;

        let wasm_dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "wasm_dealloc")
            .map_err(|e| WasmHostError::MissingExport(format!("wasm_dealloc: {e}")))?;

        let wasm_margin_preview = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "wasm_margin_preview")
            .map_err(|e| WasmHostError::MissingExport(format!("wasm_margin_preview: {e}")))?;

        let input_bytes = input_json.as_bytes();
        let input_len = input_bytes.len() as i32;

        // Step 1: Allocate input buffer in WASM memory
        let input_ptr = wasm_alloc
            .call(&mut store, input_len)
            .map_err(|e| WasmHostError::MemoryAllocation(format!("alloc input: {e}")))?;

        if input_ptr == 0 {
            return Err(WasmHostError::MemoryAllocation(
                "wasm_alloc returned null for input".to_owned(),
            ));
        }

        // Step 2: Write JSON input bytes to WASM memory
        memory
            .write(&mut store, input_ptr as usize, input_bytes)
            .map_err(|e| WasmHostError::MemoryAccess(format!("write input: {e}")))?;

        // Step 3: Call wasm_margin_preview
        let result_ptr = wasm_margin_preview
            .call(&mut store, (input_ptr, input_len))
            .map_err(|e| WasmHostError::Execution(e.to_string()))?;

        if result_ptr == 0 {
            // Deallocate input before returning error
            let _ = wasm_dealloc.call(&mut store, (input_ptr, input_len));
            return Err(WasmHostError::Execution(
                "wasm_margin_preview returned null".to_owned(),
            ));
        }

        // Step 4: Read result — [4 bytes: u32 LE length][N bytes: JSON]
        let mut len_buf = [0u8; 4];
        memory
            .read(&store, result_ptr as usize, &mut len_buf)
            .map_err(|e| WasmHostError::MemoryAccess(format!("read result length: {e}")))?;
        let result_len = u32::from_le_bytes(len_buf) as usize;

        let mut result_buf = vec![0u8; result_len];
        memory
            .read(&store, (result_ptr as usize) + 4, &mut result_buf)
            .map_err(|e| WasmHostError::MemoryAccess(format!("read result data: {e}")))?;

        let result_json =
            std::str::from_utf8(&result_buf).map_err(|_| WasmHostError::InvalidUtf8)?;
        let result_string = result_json.to_owned();

        // Step 5: Deallocate input buffer
        let _ = wasm_dealloc.call(&mut store, (input_ptr, input_len));

        // Step 6: Deallocate result buffer (4 bytes header + JSON)
        let _ = wasm_dealloc.call(&mut store, (result_ptr, (4 + result_len) as i32));

        Ok(result_string)
    }
}

// Explicit Send + Sync — Engine and Module are both thread-safe in wasmtime.
// Store is not shared; it's created per call.
unsafe impl Send for WasmRuntime {}
unsafe impl Sync for WasmRuntime {}
