//! small trait for unwrapping (or printing) errors don't even know when they happen
//! hooks into tracing

///TODO
trait WhenEven {}

impl<T, E> WhenEven for Result<T, E> {}
