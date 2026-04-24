//! Optional [`InterpreterHost`] for embedding (Leek Wars–style fight natives, tests, etc.).

use super::error::InterpretError;
use super::value::Value;

/// Built-in `debug*` channel (after the message is rendered with Java-style `string(...)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLogKind {
    /// `debug(msg)` — neutral diagnostic.
    Info,
    /// `debugC(msg, color)` — `color` lower 24 bits are RGB (`emit_debug_log` receives them).
    Colored,
    /// `debugE(msg)` — error-styled.
    Error,
    /// `debugW(msg)` — warning-styled.
    Warning,
}

/// Whether the host consumed `debug*` output (skips default stderr line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLogHandled {
    Handled,
    NotHandled,
}

/// Hooks native calls after built-in stdlib names are tried; return `Ok(None)` to fall through.
///
/// Built-ins (`abs`, `string`, …) are resolved first. For any other name (including Leek Wars
/// natives such as `getLife`), the interpreter consults the host before returning [`InterpretError::not_callable`].
pub trait InterpreterHost {
    /// `system_log_trace` mirrors Java `EntityAI.addSystemLog` / `getErrorMessage` (tab + U+25B6 + ` AI …, line …` + newline).
    /// Pass [`None`] or `Some("")` when unavailable (host should treat like Java empty trace).
    fn call_native(
        &mut self,
        name: &str,
        args: &[Value],
        system_log_trace: Option<&str>,
    ) -> Result<Option<Value>, InterpretError>;

    /// Intercept `debug` / `debugC` / `debugE` / `debugW` after the message is formatted like `string(...)`.
    /// Default: [`DebugLogHandled::NotHandled`] (interpreter prints a prefixed line to stderr).
    /// `position` is `(file_id, leek_line)` when mirroring Java `FarmerLog.addLog` (after optional color).
    fn emit_debug_log(
        &mut self,
        kind: DebugLogKind,
        message: &str,
        color_rgb24: Option<u32>,
        position: Option<(i32, i32)>,
    ) -> Result<DebugLogHandled, InterpretError> {
        let _ = (kind, message, color_rgb24, position);
        Ok(DebugLogHandled::NotHandled)
    }

    /// Extra VM operations for a Leek Wars native after a successful host dispatch (Java `FightFunctions` / `LeekFunctions` registry cost).
    #[inline]
    fn leek_fight_registry_ops(&self, _name: &str) -> u64 {
        0
    }

    /// Java wraps some natives with an extra `ai.ops(n)` inside `*Class` (not part of `getOperations()` / stmt `ops(expr, …)`).
    /// Charged after a successful [`Self::call_native`] when the interpreter applies host dispatch.
    #[inline]
    fn java_native_wrapper_ops(&self, _name: &str) -> u64 {
        0
    }

    /// Extra VM operations charged after a successful [`Self::call_native`] (e.g. Java `EntityAI.addSystemLog` → `opsNoCheck(ERROR_LOG_COST)`).
    /// Implementations should return the accumulated value once and reset their buffer.
    #[inline]
    fn take_native_dispatch_extra_ops(&mut self) -> u64 {
        0
    }
}
