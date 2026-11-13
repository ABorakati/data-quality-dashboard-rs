//! SQLite's `SQLITE_ENABLE_MATH_FUNCTIONS` functions (src/func.c in 3.50), which sqlite-jdbc's build enables
//! but `libsqlite3-sys`'s bundled build does not. Each callback mirrors the C implementation.

use rusqlite::ffi;
use std::ffi::{c_char, c_int, c_void};

type Sqlite3Value = *mut ffi::sqlite3_value;
type Context = *mut ffi::sqlite3_context;

unsafe extern "C" {
    fn acosh(x: f64) -> f64;
    fn asinh(x: f64) -> f64;
    fn atanh(x: f64) -> f64;
}

fn is_numeric(value: Sqlite3Value) -> bool {
    let kind = unsafe { ffi::sqlite3_value_numeric_type(value) };
    kind == ffi::SQLITE_INTEGER || kind == ffi::SQLITE_FLOAT
}

unsafe fn user_fn1(ctx: Context) -> fn(f64) -> f64 {
    unsafe { std::mem::transmute::<*mut c_void, fn(f64) -> f64>(ffi::sqlite3_user_data(ctx)) }
}

unsafe fn user_fn2(ctx: Context) -> fn(f64, f64) -> f64 {
    unsafe { std::mem::transmute::<*mut c_void, fn(f64, f64) -> f64>(ffi::sqlite3_user_data(ctx)) }
}

/// `ceil`, `ceiling`, `floor`, `trunc`: integers pass through unchanged.
unsafe extern "C" fn ceiling_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
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
    unsafe {
        let arg = *argv;
        if !is_numeric(arg) {
            return;
        }
        ffi::sqlite3_result_double(ctx, user_fn1(ctx)(ffi::sqlite3_value_double(arg)));
    }
}

unsafe extern "C" fn math2_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
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

unsafe extern "C" fn log_func(ctx: Context, argc: c_int, argv: *mut Sqlite3Value) {
    unsafe {
        let first = *argv;
        if !is_numeric(first) {
            return;
        }
        let mut x = ffi::sqlite3_value_double(first);
        if x <= 0.0 {
            return;
        }
        let answer;
        if argc == 2 {
            let b = x.ln();
            if b <= 0.0 {
                return;
            }
            let second = *argv.add(1);
            if !is_numeric(second) {
                return;
            }
            x = ffi::sqlite3_value_double(second);
            if x <= 0.0 {
                return;
            }
            answer = x.ln() / b;
        } else {
            answer = match ffi::sqlite3_user_data(ctx) as usize {
                1 => x.ln() / std::f64::consts::LN_10,
                2 => x.ln() / std::f64::consts::LN_2,
                _ => x.ln(),
            };
        }
        ffi::sqlite3_result_double(ctx, answer);
    }
}

unsafe extern "C" fn pi_func(ctx: Context, _argc: c_int, _argv: *mut Sqlite3Value) {
    unsafe { ffi::sqlite3_result_double(ctx, std::f64::consts::PI) }
}

unsafe extern "C" fn sign_func(ctx: Context, _argc: c_int, argv: *mut Sqlite3Value) {
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

unsafe fn register(
    db: *mut ffi::sqlite3,
    name: &str,
    args: c_int,
    user: *mut c_void,
    callback: Callback,
) {
    let mut c_name = name.as_bytes().to_vec();
    c_name.push(0);
    unsafe {
        ffi::sqlite3_create_function_v2(
            db,
            c_name.as_ptr() as *const c_char,
            args,
            ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
            user,
            Some(callback),
            None,
            None,
            None,
        );
    }
}

/// Register every function `SQLITE_ENABLE_MATH_FUNCTIONS` adds.
pub unsafe fn register_math_functions(db: *mut ffi::sqlite3) {
    fn f1(function: fn(f64) -> f64) -> *mut c_void {
        function as usize as *mut c_void
    }
    fn f2(function: fn(f64, f64) -> f64) -> *mut c_void {
        function as usize as *mut c_void
    }
    unsafe {
        register(db, "ceil", 1, f1(f64::ceil), ceiling_func);
        register(db, "ceiling", 1, f1(f64::ceil), ceiling_func);
        register(db, "floor", 1, f1(f64::floor), ceiling_func);
        register(db, "trunc", 1, f1(f64::trunc), ceiling_func);
        register(db, "ln", 1, f1(f64::ln), math1_func);
        register(db, "log", 1, 1usize as *mut c_void, log_func);
        register(db, "log10", 1, 1usize as *mut c_void, log_func);
        register(db, "log2", 1, 2usize as *mut c_void, log_func);
        register(db, "log", 2, std::ptr::null_mut(), log_func);
        register(db, "exp", 1, f1(f64::exp), math1_func);
        register(db, "pow", 2, f2(f64::powf), math2_func);
        register(db, "power", 2, f2(f64::powf), math2_func);
        register(db, "mod", 2, f2(|a, b| a % b), math2_func);
        register(db, "acos", 1, f1(f64::acos), math1_func);
        register(db, "asin", 1, f1(f64::asin), math1_func);
        register(db, "atan", 1, f1(f64::atan), math1_func);
        register(db, "atan2", 2, f2(f64::atan2), math2_func);
        register(db, "cos", 1, f1(f64::cos), math1_func);
        register(db, "sin", 1, f1(f64::sin), math1_func);
        register(db, "tan", 1, f1(f64::tan), math1_func);
        register(db, "cosh", 1, f1(f64::cosh), math1_func);
        register(db, "sinh", 1, f1(f64::sinh), math1_func);
        register(db, "tanh", 1, f1(f64::tanh), math1_func);
        register(db, "acosh", 1, f1(|x| acosh(x)), math1_func);
        register(db, "asinh", 1, f1(|x| asinh(x)), math1_func);
        register(db, "atanh", 1, f1(|x| atanh(x)), math1_func);
        register(db, "sqrt", 1, f1(f64::sqrt), math1_func);
        register(
            db,
            "radians",
            1,
            f1(|x| x * (std::f64::consts::PI / 180.0)),
            math1_func,
        );
        register(
            db,
            "degrees",
            1,
            f1(|x| x * (180.0 / std::f64::consts::PI)),
            math1_func,
        );
        register(db, "pi", 0, std::ptr::null_mut(), pi_func);
        register(db, "sign", 1, std::ptr::null_mut(), sign_func);
    }
}

/// The bytes of a value converted to text, as `sqlite3_value_text` gives them.
unsafe fn value_text<'a>(value: Sqlite3Value) -> &'a [u8] {
    unsafe {
        let text = ffi::sqlite3_value_text(value);
        let len = ffi::sqlite3_value_bytes(value) as usize;
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
    unsafe {
        let args = std::slice::from_raw_parts(argv, argc as usize);
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
        if !needle.is_empty() && characters == skip {
            if let Some(found) = haystack[offset..]
                .windows(needle.len())
                .position(|window| window == needle)
            {
                let before = haystack[..offset + found]
                    .iter()
                    .filter(|&&b| is_start(b))
                    .count();
                position = before as c_int + 1;
            }
        }
        ffi::sqlite3_result_int(ctx, position);
    }
}

/// Register the RSQLite extension functions that DQD's translated SQL uses.
pub unsafe fn register_extension_functions(db: *mut ffi::sqlite3) {
    unsafe {
        register(db, "charindex", 2, std::ptr::null_mut(), charindex_func);
        register(db, "charindex", 3, std::ptr::null_mut(), charindex_func);
    }
}
