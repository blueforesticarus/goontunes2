//! small trait for unwrapping (or printing) errors don't even know when they happen
//! hooks into tracing

use std::{
    backtrace::Backtrace,
    error::Error,
    fmt::{Debug, Display},
    path::{Path, PathBuf},
};

use tracing::error;

pub trait Ignoreable: Sized {
    fn ignore(self) {}
}
impl<T, E> Ignoreable for Result<T, E> {}

pub trait Logger<T: ?Sized> {
    fn log(value: &T);
}

pub struct Bug;
impl<T, E: Debug> Logger<Result<T, E>> for Bug {
    fn log(value: &Result<T, E>) {
        match value {
            Ok(_) => {}
            Err(e) => error!("{:?}, {}", e, Backtrace::capture()),
        }
    }
}
pub struct OnError;
impl<T, E: Debug> Logger<Result<T, E>> for OnError {
    fn log(value: &Result<T, E>) {
        match value {
            Ok(_) => {}
            Err(e) => error!("{:?}", e),
        }
    }
}

pub trait Loggable: Sized {
    fn log<L>(self) -> Self
    where
        L: Logger<Self>,
    {
        L::log(&self);
        self
    }

    fn log_and_drop<L: Logger<Self>>(self) {
        self.log::<L>();
    }
}

impl<V, E: Display> Loggable for Result<V, E> {}

#[derive(Debug, Clone)]
pub struct Context<E, C>(E, C);

impl<E: Display, C: Display> Display for Context<E, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\nContext:\n{}", self.0, self.1)
    }
}

impl<E, C> Error for Context<E, C> where Context<E, C>: Display + Debug {}

pub trait WithContext {
    type Context: Debug;

    fn contextualize<E, L>(
        &self,
        f: impl FnOnce() -> Result<L, E>,
    ) -> Result<L, Context<E, Self::Context>> {
        f().map_err(|e| Context(e, self.to_context()))
    }

    fn to_context(&self) -> Self::Context;
}

impl WithContext for PathBuf {
    type Context = NamedContext<PathBuf>;

    fn to_context(&self) -> Self::Context {
        NamedContext("path".to_string(), self.to_path_buf())
    }
}

impl WithContext for Path {
    type Context = <PathBuf as WithContext>::Context;

    fn to_context(&self) -> Self::Context {
        self.to_path_buf().to_context()
    }
}

#[derive(Debug, Clone)]
pub struct NamedContext<B>(String, B);

impl<B: Debug> Display for NamedContext<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {:?}", self.0, self.1)
    }
}

impl<A: ToString, B: WithContext + ?Sized> WithContext for (A, &B) {
    type Context = NamedContext<B::Context>;

    fn to_context(&self) -> Self::Context {
        NamedContext(self.0.to_string(), self.1.to_context())
    }
}

#[derive(Debug, Clone)]
pub struct End;

impl WithContext for End {
    type Context = End;
    fn to_context(&self) -> Self::Context {
        End
    }
}

impl WithContext for str {
    type Context = String;

    fn to_context(&self) -> Self::Context {
        self.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct ContextBuilder<'a, T: WithContext + ?Sized, Next: WithContext = End>(&'a T, Next);

pub fn with<T: WithContext + ?Sized>(v: &T) -> ContextBuilder<T> {
    ContextBuilder(v, End)
}

impl<'a, T: WithContext, N: WithContext> ContextBuilder<'a, T, N> {
    pub fn with<T2: WithContext + ?Sized>(self, v: &'a T2) -> ContextBuilder<'a, T2, Self> {
        ContextBuilder(v, self)
    }
}

#[derive(Debug, Clone)]
pub struct JoinedContext<A, B>(A, B);

impl<'a, T: WithContext + ?Sized, Next: WithContext> WithContext for ContextBuilder<'a, T, Next> {
    type Context = JoinedContext<T::Context, Next::Context>;

    fn to_context(&self) -> Self::Context {
        JoinedContext(self.0.to_context(), self.1.to_context())
    }
}

impl<A: Display, B: Display> Display for JoinedContext<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\t{}\n{}", self.0, self.1)
    }
}

impl<A: Display> Display for JoinedContext<A, End> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\t{}\n", self.0)
    }
}
