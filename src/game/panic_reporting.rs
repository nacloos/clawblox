use std::any::Any;
use std::backtrace::Backtrace;

pub fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        (*msg).to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

pub fn log_panic(component: &str, context: &str, payload: &(dyn Any + Send)) {
    let message = panic_payload_message(payload);
    let backtrace = Backtrace::force_capture();
    eprintln!(
        "[FATAL][{}] {} panic=\"{}\"\n{}",
        component, context, message, backtrace
    );
}
