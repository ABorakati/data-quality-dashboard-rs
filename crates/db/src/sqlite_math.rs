//! SQLite's `SQLITE_ENABLE_MATH_FUNCTIONS` functions (src/func.c in 3.50), which sqlite-jdbc's build enables
//! but `libsqlite3-sys`'s bundled build does not. Each callback mirrors the C implementation.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

// Contract shared by every `unsafe extern "C"` callback below: SQLite calls them with a valid
// `sqlite3_context` and an `argv` array holding exactly as many valid `sqlite3_value` pointers as
// the argument count the function was registered with (`argc`). `register` is the only place that
// registers them, and each registration pairs a callback with its arity.

use rusqlite::ffi;
use std::ffi::{CStr, c_int, c_void};

type Sqlite3Value = *mut ffi::sqlite3_value;
type Context = *mut ffi::sqlite3_context;

unsafe extern "C" {
    // libm: SQLite's own build calls these, and std does not expose bit-identical equivalents.
    safe fn acosh(x: f64) -> f64;
    safe fn asinh(x: f64) -> f64;
    safe fn atanh(x: f64) -> f64;
}

fn is_numeric(value: Sqlite3Value) -> bool {
    // SAFETY: `value` is one of the argument values SQLite passed to the running callback.
    let kind = unsafe { ffi::sqlite3_value_numeric_type(value) };
    kind == ffi::SQLITE_INTEGER || kind == ffi::SQLITE_FLOAT
}

/// The one-argument function stored as user data by `register_math_functions`.
///
/// # Safety
/// `ctx` must belong to a function registered with a `fn(f64) -> f64` as its user data.
unsafe fn user_fn1(ctx: Context) -> fn(f64) -> f64 {
    // SAFETY: per the function contract the user data is a non-null `fn(f64) -> f64` cast to a data
    // pointer by `f1`; both types are pointer-sized.
    unsafe { std::mem::transmute::<*mut c_void, fn(f64) -> f64>(ffi::sqlite3_user_data(ctx)) }
}

/// The two-argument function stored as user data by `register_math_functions`.
///
/// # Safety
/// `ctx` must belong to a function registered with a `fn(f64, f64) -> f64` as its user data.
unsafe fn user_fn2(ctx: Context) -> fn(f64, f64) -> f64 {
    // SAFETY: as for `user_fn1`, with the two-argument signature.
    unsafe { std::mem::transmute::<*mut c_void, fn(f64, f64) -> f64>(ffi::sqlite3_user_data(ctx)) }
}

/// `ceil`, `ceiling`, `floor`, `trunc`: integers pass through unchanged.
unsafe extern "C" fn ceiling_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with one argument, so `argv[0]` is valid, and the user data is a
    // `fn(f64) -> f64` (see the contract above).
    unsafe {
        let arg = *argv;
        match ffi::sqlite3_value_numeric_type(arg) {
            ffi::SQLITE_INTEGER => ffi::sqlite3_result_int64(ctx, ffi::sqlite3_value_int64(arg)),
            ffi::SQLITE_FLOAT => {
                ffi::sqlite3_result_double(ctx, user_fn1(ctx)(ffi::sqlite3_value_double(arg)))
            }
            _ => {}
        }
    }
}

unsafe extern "C" fn math1_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with one argument and a `fn(f64) -> f64` user data.
    unsafe {
        let arg = *argv;
        if !is_numeric(arg) {
            return;
        }
        ffi::sqlite3_result_double(ctx, user_fn1(ctx)(ffi::sqlite3_value_double(arg)));
    }
}

unsafe extern "C" fn math2_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with two arguments and a `fn(f64, f64) -> f64` user data.
    unsafe {
        let (a, b) = (*argv, *argv.add(1));
        if !is_numeric(a) || !is_numeric(b) {
            return;
        }
        ffi::sqlite3_result_double(
            ctx,
            user_fn2(ctx)(ffi::sqlite3_value_double(a), ffi::sqlite3_value_double(b)),
        );
    }
}

/// `log(X)`, `log10(X)` and `log2(X)`: the natural logarithm of the argument divided by `divisor`.
///
/// # Safety
/// `argv` must point to at least one valid value.
unsafe fn log_unary(ctx: Context, argv: *mut Sqlite3Value, divisor: f64) {
    // SAFETY: the caller guarantees `argv[0]` is valid.
    unsafe {
        let first = *argv;
        if !is_numeric(first) {
            return;
        }
        let x = ffi::sqlite3_value_double(first);
        if x <= 0.0 {
            return;
        }
        ffi::sqlite3_result_double(ctx, x.ln() / divisor);
    }
}

unsafe extern "C" fn log10_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with one argument.
    unsafe { log_unary(ctx, argv, std::f64::consts::LN_10) }
}

unsafe extern "C" fn log2_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with one argument.
    unsafe { log_unary(ctx, argv, std::f64::consts::LN_2) }
}

/// `log(B, X)`: the logarithm of `X` to base `B`.
unsafe extern "C" fn log_base_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with two arguments, so `argv[0]` and `argv[1]` are valid.
    unsafe {
        let (first, second) = (*argv, *argv.add(1));
        if !is_numeric(first) {
            return;
        }
        let base = ffi::sqlite3_value_double(first);
        if base <= 0.0 {
            return;
        }
        let base_ln = base.ln();
        if base_ln <= 0.0 || !is_numeric(second) {
            return;
        }
        let x = ffi::sqlite3_value_double(second);
        if x <= 0.0 {
            return;
        }
        ffi::sqlite3_result_double(ctx, x.ln() / base_ln);
    }
}

unsafe extern "C" fn pi_func(ctx: Context, _argc: c_int, _argv: *mut Sqlite3Value) {
    // SAFETY: `ctx` is valid (see the contract above); no argument is read.
    unsafe { ffi::sqlite3_result_double(ctx, std::f64::consts::PI) }
}

unsafe extern "C" fn sign_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: registered with one argument.
    unsafe {
        let arg = *argv;
        if !is_numeric(arg) {
            return;
        }
        let x = ffi::sqlite3_value_double(arg);
        ffi::sqlite3_result_int(
            ctx,
            if x > 0.0 {
                1
            } else if x < 0.0 {
                -1
            } else {
                0
            },
        );
    }
}

type Callback = unsafe extern "C" fn(Context, c_int, *mut Sqlite3Value);

/// Register one deterministic UTF-8 scalar function; the error is SQLite's result code.
///
/// # Safety
/// `db` must be an open connection, `callback` must follow the contract above for `args`
/// arguments, and `user` must be whatever `callback` expects from `sqlite3_user_data` (it is never
/// freed, so it must not own memory).
unsafe fn register(
    db: *mut ffi::sqlite3,
    name: &CStr,
    args: c_int,
    user: *mut c_void,
    callback: Callback,
) -> Result<(), c_int> {
    // SAFETY: `name` is NUL-terminated and SQLite copies it; the remaining arguments are covered by
    // the function's contract. No destructor is registered because `user` owns nothing.
    let rc = unsafe {
        ffi::sqlite3_create_function_v2(
            db,
            name.as_ptr(),
            args,
            ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
            user,
            Some(callback),
            None,
            None,
            None,
        )
    };
    if rc == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

/// Register every function `SQLITE_ENABLE_MATH_FUNCTIONS` adds. On failure the error is the
/// SQLite result code of the first registration that failed.
///
/// # Safety
/// `db` must be an open SQLite connection.
pub(crate) unsafe fn register_math_functions(db: *mut ffi::sqlite3) -> Result<(), c_int> {
    fn f1(function: fn(f64) -> f64) -> *mut c_void {
        function as *mut c_void
    }
    fn f2(function: fn(f64, f64) -> f64) -> *mut c_void {
        function as *mut c_void
    }
    let none = std::ptr::null_mut();
    // SAFETY: `db` is open by this function's contract. Each callback is paired with its arity and
    // user data: the one- and two-argument callbacks get a function pointer from `f1`/`f2` of the
    // matching signature, and the others use no user data.
    unsafe {
        register(db, c"ceil", 1, f1(f64::ceil), ceiling_func)?;
        register(db, c"ceiling", 1, f1(f64::ceil), ceiling_func)?;
        register(db, c"floor", 1, f1(f64::floor), ceiling_func)?;
        register(db, c"trunc", 1, f1(f64::trunc), ceiling_func)?;
        register(db, c"ln", 1, f1(f64::ln), math1_func)?;
        register(db, c"log", 1, none, log10_func)?;
        register(db, c"log10", 1, none, log10_func)?;
        register(db, c"log2", 1, none, log2_func)?;
        register(db, c"log", 2, none, log_base_func)?;
        register(db, c"exp", 1, f1(f64::exp), math1_func)?;
        register(db, c"pow", 2, f2(f64::powf), math2_func)?;
        register(db, c"power", 2, f2(f64::powf), math2_func)?;
        register(db, c"mod", 2, f2(|a, b| a % b), math2_func)?;
        register(db, c"acos", 1, f1(f64::acos), math1_func)?;
        register(db, c"asin", 1, f1(f64::asin), math1_func)?;
        register(db, c"atan", 1, f1(f64::atan), math1_func)?;
        register(db, c"atan2", 2, f2(f64::atan2), math2_func)?;
        register(db, c"cos", 1, f1(f64::cos), math1_func)?;
        register(db, c"sin", 1, f1(f64::sin), math1_func)?;
        register(db, c"tan", 1, f1(f64::tan), math1_func)?;
        register(db, c"cosh", 1, f1(f64::cosh), math1_func)?;
        register(db, c"sinh", 1, f1(f64::sinh), math1_func)?;
        register(db, c"tanh", 1, f1(f64::tanh), math1_func)?;
        register(db, c"acosh", 1, f1(|x| acosh(x)), math1_func)?;
        register(db, c"asinh", 1, f1(|x| asinh(x)), math1_func)?;
        register(db, c"atanh", 1, f1(|x| atanh(x)), math1_func)?;
        register(db, c"sqrt", 1, f1(f64::sqrt), math1_func)?;
        register(
            db,
            c"radians",
            1,
            f1(|x| x * (std::f64::consts::PI / 180.0)),
            math1_func,
        )?;
        register(
            db,
            c"degrees",
            1,
            f1(|x| x * (180.0 / std::f64::consts::PI)),
            math1_func,
        )?;
        register(db, c"pi", 0, none, pi_func)?;
        register(db, c"sign", 1, none, sign_func)?;
    }
    Ok(())
}

/// The bytes of a value converted to text, as `sqlite3_value_text` gives them.
///
/// # Safety
/// `value` must be a valid argument value, and the result must not outlive the running callback
/// nor any later type conversion of that same value (the conversion may free the buffer).
unsafe fn value_text<'a>(value: Sqlite3Value) -> &'a [u8] {
    // SAFETY: `sqlite3_value_text` is called before `sqlite3_value_bytes`, as SQLite requires, and
    // returns either NULL or a buffer of that many bytes that stays valid under the caller's
    // contract.
    unsafe {
        let text = ffi::sqlite3_value_text(value);
        let len = usize::try_from(ffi::sqlite3_value_bytes(value)).unwrap_or(0);
        if text.is_null() || len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(text, len)
        }
    }
}

/// `charindex(needle, haystack [, start])` from RSQLite's bundled `extension-functions.c`
/// (RSQLite loads it on every connection, so DatabaseConnector's SQLite has it and DQD's SQLite
/// SQL relies on it). The result is the 1-based character position of the first match at or after
/// `start`, or 0. NULL needle or haystack gives NULL; an empty needle gives 0; `start` below 1
/// (including a NULL `start`) searches from the beginning.
unsafe extern "C" fn charindex_func(ctx: Context, argc: c_int, argv: *mut Sqlite3Value) {
    // SAFETY: `argv` holds `argc` valid values (see the contract above). Both text buffers belong
    // to different values, so reading one cannot invalidate the other, and neither outlives this call.
    unsafe {
        let args = std::slice::from_raw_parts(argv, usize::try_from(argc).unwrap_or(0));
        if args.len() < 2 {
            return;
        }
        if ffi::sqlite3_value_type(args[0]) == ffi::SQLITE_NULL
            || ffi::sqlite3_value_type(args[1]) == ffi::SQLITE_NULL
        {
            return;
        }
        let needle = value_text(args[0]);
        let haystack = value_text(args[1]);
        let skip = if argc == 3 {
            (ffi::sqlite3_value_int(args[2]).max(1) - 1) as usize
        } else {
            0
        };
        let is_start = |byte: u8| byte & 0xC0 != 0x80;
        // Byte offset of the character `skip` characters in; past the end nothing can match.
        let mut offset = 0;
        let mut characters = 0;
        while characters < skip && offset < haystack.len() {
            offset += 1;
            while offset < haystack.len() && !is_start(haystack[offset]) {
                offset += 1;
            }
            characters += 1;
        }
        let mut position = 0;
        if !needle.is_empty()
            && characters == skip
            && let Some(found) = haystack[offset..]
                .windows(needle.len())
                .position(|window| window == needle)
        {
            let before = haystack[..offset + found]
                .iter()
                .filter(|&&b| is_start(b))
                .count();
            position = before as c_int + 1;
        }
        ffi::sqlite3_result_int(ctx, position);
    }
}

/// Register the RSQLite extension functions that DQD's translated SQL uses. On failure the error is
/// the SQLite result code.
///
/// # Safety
/// `db` must be an open SQLite connection.
pub(crate) unsafe fn register_extension_functions(db: *mut ffi::sqlite3) -> Result<(), c_int> {
    // SAFETY: `db` is open by this function's contract; `charindex_func` accepts two or three
    // arguments and uses no user data.
    unsafe {
        register(db, c"charindex", 2, std::ptr::null_mut(), charindex_func)?;
        register(db, c"charindex", 3, std::ptr::null_mut(), charindex_func)
    }
}
