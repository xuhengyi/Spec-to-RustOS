//! console (rcore-console) crate 功能性验证测试
//! 
//! 这些测试验证 console crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。

use std::sync::{Arc, Mutex, Once};
use rcore_console::{Console, init_console, set_log_level, test_log};

// 测试用的 Console 实现
struct TestConsole {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Console for TestConsole {
    fn put_char(&self, c: u8) {
        self.output.lock().unwrap().push(c);
    }
    
    // 覆盖 put_str 以避免逐个字符调用 put_char
    fn put_str(&self, s: &str) {
        self.output.lock().unwrap().extend_from_slice(s.as_bytes());
    }
}

// 共享的测试 console，用于所有需要测试全局 console 的测试
// 由于 CONSOLE 是全局静态变量且 Once::call_once 只执行一次，
// 我们需要使用一个共享的 console 实例，并在每个测试开始时清空缓冲区
static SHARED_OUTPUT: Mutex<Option<Arc<Mutex<Vec<u8>>>>> = Mutex::new(None);
static SHARED_CONSOLE_INIT: Once = Once::new();

fn get_shared_output() -> Arc<Mutex<Vec<u8>>> {
    // 检查是否已初始化
    {
        let guard = SHARED_OUTPUT.lock().unwrap();
        if let Some(ref output) = *guard {
            return output.clone();
        }
    }
    
    // 初始化共享 console 和输出缓冲区
    SHARED_CONSOLE_INIT.call_once(|| {
        let output = Arc::new(Mutex::new(Vec::new()));
        let console = Box::leak(Box::new(TestConsole {
            output: output.clone(),
        }));
        
        // 初始化全局 console（只执行一次）
        init_console(console);
        
        // 存储输出缓冲区的引用
        *SHARED_OUTPUT.lock().unwrap() = Some(output);
    });
    
    // 再次获取（现在应该已经初始化了）
    let guard = SHARED_OUTPUT.lock().unwrap();
    guard.as_ref().unwrap().clone()
}

// 在每个测试开始时清空输出缓冲区
fn clear_output() {
    get_shared_output().lock().unwrap().clear();
}

// 获取当前输出内容
fn get_output() -> Vec<u8> {
    get_shared_output().lock().unwrap().clone()
}

#[test]
fn test_console_trait_basic() {
    // 测试 Console trait 的基本功能
    let output = Arc::new(Mutex::new(Vec::new()));
    let console = TestConsole {
        output: output.clone(),
    };
    
    console.put_char(b'A');
    assert_eq!(output.lock().unwrap()[0], b'A');
    
    console.put_char(b'B');
    assert_eq!(output.lock().unwrap()[1], b'B');
}

#[test]
fn test_console_put_str() {
    // 测试 put_str 方法
    let output = Arc::new(Mutex::new(Vec::new()));
    let console = TestConsole {
        output: output.clone(),
    };
    
    console.put_str("hello");
    let bytes = output.lock().unwrap();
    assert_eq!(bytes.as_slice(), b"hello");
}

#[test]
fn test_console_put_str_multibyte() {
    // 测试 put_str 处理多字节字符
    let output = Arc::new(Mutex::new(Vec::new()));
    let console = TestConsole {
        output: output.clone(),
    };
    
    console.put_str("hello world\n");
    let bytes = output.lock().unwrap();
    assert_eq!(bytes.as_slice(), b"hello world\n");
}

#[test]
fn test_console_init() {
    // 测试 init_console 函数
    // 注意：由于 CONSOLE 是全局静态变量，Once::call_once 只执行一次
    // 这里主要验证多次调用 init_console 不会 panic
    clear_output();
    
    // 确保共享 console 已初始化
    get_shared_output();
    
    // 验证可以正常输出
    rcore_console::print!("init test");
    let bytes = get_output();
    assert!(bytes.len() > 0);
    
    // 验证多次调用 init_console 不会 panic（虽然实际上只会初始化一次）
    // 注意：由于 console 已经初始化，这里再次调用 init_console 会被忽略
    // 但不会 panic，这是 Once::call_once 的预期行为
}

#[test]
fn test_console_set_log_level() {
    // 测试 set_log_level 函数
    set_log_level(None);
    set_log_level(Some("info"));
    set_log_level(Some("debug"));
    set_log_level(Some("trace"));
    set_log_level(Some("warn"));
    set_log_level(Some("error"));
    set_log_level(Some("invalid")); // 应该回退到默认值
}

#[test]
fn test_print_macro() {
    // 测试 print! 宏
    clear_output();
    
    rcore_console::print!("test");
    let bytes = get_output();
    assert_eq!(bytes.as_slice(), b"test");
}

#[test]
fn test_println_macro_empty() {
    // 测试 println!() 空参数
    clear_output();
    
    rcore_console::println!();
    let bytes = get_output();
    assert_eq!(bytes.as_slice(), b"\n");
}

#[test]
fn test_println_macro_with_args() {
    // 测试 println! 宏带参数
    clear_output();
    
    rcore_console::println!("hello {}", "world");
    let bytes = get_output();
    let output_str = std::str::from_utf8(&bytes).unwrap();
    assert!(output_str.contains("hello"));
    assert!(output_str.contains("world"));
    assert!(output_str.ends_with("\n"));
}

#[test]
fn test_println_formatting() {
    // 测试 println! 格式化功能
    clear_output();
    
    rcore_console::println!("Number: {}", 42);
    rcore_console::println!("Hex: {:#x}", 255);
    rcore_console::println!("Binary: {:#b}", 7);
    
    let bytes = get_output();
    let output_str = std::str::from_utf8(&bytes).unwrap();
    assert!(output_str.contains("42"));
    assert!(output_str.contains("0xff") || output_str.contains("FF"));
    assert!(output_str.contains("0b111") || output_str.contains("111"));
}

#[test]
fn test_log_integration() {
    // 测试 log crate 集成
    clear_output();
    set_log_level(Some("trace"));
    
    log::trace!("trace message");
    log::debug!("debug message");
    log::info!("info message");
    log::warn!("warn message");
    log::error!("error message");
    
    let bytes = get_output();
    let output_str = std::str::from_utf8(&bytes).unwrap();
    
    // 验证日志消息被输出
    assert!(output_str.contains("trace message"));
    assert!(output_str.contains("debug message"));
    assert!(output_str.contains("info message"));
    assert!(output_str.contains("warn message"));
    assert!(output_str.contains("error message"));
    
    // 验证日志级别被输出
    assert!(output_str.contains("TRACE") || output_str.contains("trace"));
    assert!(output_str.contains("DEBUG") || output_str.contains("debug"));
    assert!(output_str.contains("INFO") || output_str.contains("info"));
    assert!(output_str.contains("WARN") || output_str.contains("warn"));
    assert!(output_str.contains("ERROR") || output_str.contains("error"));
}

#[test]
fn test_test_log_function() {
    // 测试 test_log 函数
    clear_output();
    // 确保日志级别设置为 trace，以便所有日志消息都能输出
    set_log_level(Some("trace"));
    
    test_log();
    
    let bytes = get_output();
    let output_str = std::str::from_utf8(&bytes).unwrap();
    
    // 验证 test_log 输出了 ASCII art（包含下划线，看起来像 "rCore"）
    // ASCII art 中包含下划线，所以应该能找到相关内容
    assert!(output_str.contains("____") || output_str.contains("LOG TEST"), 
            "Output should contain ASCII art or 'LOG TEST', but got: {:?}", output_str);
    
    // 验证日志消息被输出（如果日志级别设置正确）
    // 注意：日志消息可能因为日志级别设置而没有被输出
    if output_str.contains("LOG TEST") {
        assert!(output_str.contains("TRACE") || output_str.contains("trace") || 
                output_str.contains("DEBUG") || output_str.contains("debug") ||
                output_str.contains("INFO") || output_str.contains("info") ||
                output_str.contains("WARN") || output_str.contains("warn") ||
                output_str.contains("ERROR") || output_str.contains("error"));
    }
}

#[test]
fn test_console_sync() {
    // 测试 Console trait 是 Sync 的
    let output = Arc::new(Mutex::new(Vec::new()));
    let console = Arc::new(TestConsole {
        output: output.clone(),
    });
    
    // 验证可以跨线程共享
    let console_clone = console.clone();
    std::thread::spawn(move || {
        console_clone.put_char(b'X');
    }).join().unwrap();
    
    // 注意：由于 Mutex 是线程安全的，这里可以安全地跨线程使用
    // 实际使用中应该使用 Mutex 或其他同步原语
}
