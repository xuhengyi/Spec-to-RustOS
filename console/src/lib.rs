//! rcore-console: 提供可定制实现的 `print!`、`println!` 与 `log::Log`

#![no_std]

pub extern crate log;

use core::fmt;
use log::{Level, LevelFilter, Log, Metadata, Record};
use spin::Once;

/// 控制台输出抽象 trait
/// 
/// 实现者必须提供 `put_char` 方法以输出单个字节。
/// 默认的 `put_str` 实现会逐字节调用 `put_char`。
pub trait Console: Sync {
    /// 输出单个字节
    fn put_char(&self, c: u8);
    
    /// 输出字符串（默认实现逐字节调用 `put_char`）
    fn put_str(&self, s: &str) {
        for byte in s.bytes() {
            self.put_char(byte);
        }
    }
}

/// 全局控制台单例
static CONSOLE: Once<&'static dyn Console> = Once::new();

/// 初始化全局控制台单例并注册 logger
/// 
/// # 参数
/// * `console` - 静态控制台实现引用
/// 
/// # 行为
/// - 首次调用会设置全局控制台单例
/// - 注册全局 logger
/// - 重复调用可能 panic（因为 logger 只能注册一次，当前实现会忽略重复注册）
pub fn init_console(console: &'static dyn Console) {
    CONSOLE.call_once(|| console);
    // 如果 logger 已经注册，忽略错误（符合 spec：重复调用可能 panic，但不是必须）
    let _ = log::set_logger(&Logger);
}

/// 设置全局最大日志级别
/// 
/// # 参数
/// * `env` - 日志级别字符串（如 "trace", "debug", "info", "warn", "error"）
///         如果为 `None` 或无法解析，则设置为 `Trace`
pub fn set_log_level(env: Option<&str>) {
    let level = if let Some(env_str) = env {
        // 手动转换为小写进行比较（no_std 环境）
        let mut lower = [0u8; 16];
        let mut i = 0;
        for byte in env_str.bytes().take(15) {
            lower[i] = if byte >= b'A' && byte <= b'Z' {
                byte + 32
            } else {
                byte
            };
            i += 1;
        }
        let lower_str = core::str::from_utf8(&lower[..i]).unwrap_or("");
        match lower_str {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => LevelFilter::Trace,
        }
    } else {
        LevelFilter::Trace
    };
    log::set_max_level(level);
}

/// 输出测试 banner 和五条不同级别的日志
pub fn test_log() {
    println!(r#"
 ____  ____                    _       _   _      _   
|  _ \|  _ \  ___  _ __   ___| |_ ___| | | | ___| |_ 
| |_) | | | |/ _ \| '_ \ / _ \ __/ _ \ | | |/ _ \ __|
|  _ <| |_| | (_) | | | |  __/ ||  __/ |_| |  __/ |_ 
|_| \_\____/ \___/|_| |_|\___|\__\___|\___/ \___|\__|
                                                      
 ____        _   _   _      ____  _____ ____  
| __ )  ___ | | | | | |_   / ___|| ____/ ___| 
|  _ \ / _ \| | | | | __|  \___ \|  _| \___ \ 
| |_) | (_) | |_| | | |_    ___) | |___ ___) |
|____/ \___/ \___/  \__|   |____/|_____|____/ 
"#);
    
    log::trace!("trace message");
    log::debug!("debug message");
    log::info!("info message");
    log::warn!("warn message");
    log::error!("error message");
    println!();
}

/// 内部打印函数，供宏使用
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let console = CONSOLE.get().expect("console not initialized");
    let mut writer = ConsoleWriter { console: *console };
    fmt::write(&mut writer, args).unwrap();
}

/// 控制台写入器，用于格式化输出
struct ConsoleWriter {
    console: &'static dyn Console,
}

impl fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.console.put_str(s);
        Ok(())
    }
}

/// 格式化颜色数字为字符串
fn format_color(mut n: u8, buf: &mut [u8; 4]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap();
    }
    let mut i = 0;
    while n > 0 && i < 3 {
        buf[2 - i] = b'0' + (n % 10);
        n /= 10;
        i += 1;
    }
    core::str::from_utf8(&buf[3 - i..]).unwrap()
}

/// Logger 实现
struct Logger;

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }
    
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        
        let level = record.level();
        let color = match level {
            Level::Error => 31,
            Level::Warn => 93,
            Level::Info => 34,
            Level::Debug => 32,
            Level::Trace => 90,
        };
        
        let level_str = match level {
            Level::Error => "ERROR",
            Level::Warn => " WARN",
            Level::Info => " INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };
        
        let console = CONSOLE.get().unwrap();
        let args = record.args();
        
        // 格式化输出: \x1b[{color}m[{level:>5}] {args}\x1b[0m\n
        console.put_str("\x1b[");
        // 手动格式化数字（color 是 u8，范围 0-255）
        let mut color_buf = [0u8; 4];
        let color_str = format_color(color, &mut color_buf);
        console.put_str(color_str);
        console.put_str("m[");
        console.put_str(level_str);
        console.put_str("] ");
        
        // 输出日志参数
        let mut writer = ConsoleWriter { console: *console };
        fmt::write(&mut writer, *args).unwrap();
        
        console.put_str("\x1b[0m\n");
    }
    
    fn flush(&self) {
        // 无需实现
    }
}

/// 格式化输出宏（无自动换行）
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*));
    };
}

/// 格式化输出宏（自动追加换行）
#[macro_export]
macro_rules! println {
    () => {
        $crate::_print(format_args!("\n"));
    };
    ($($arg:tt)*) => {
        {
            $crate::_print(format_args!($($arg)*));
            $crate::_print(format_args!("\n"));
        }
    };
}
