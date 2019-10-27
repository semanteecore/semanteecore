use failure::SyncFailure;
use serde::{Serialize, Serializer};
use std::cell::RefCell;

pub trait ResultExt<T, E> {
    fn sync(self) -> Result<T, SyncFailure<E>>
    where
        Self: Sized,
        E: ::std::error::Error + Send + 'static;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn sync(self) -> Result<T, SyncFailure<E>>
    where
        Self: Sized,
        E: ::std::error::Error + Send + 'static,
    {
        self.map_err(SyncFailure::new)
    }
}

// This serde helper struct allows to avoid collecting iterator into serde_json::Value,
// through consuming iterator in the serialization process directly
pub struct SerIter<I>(RefCell<I>);

impl<I> From<I> for SerIter<I> {
    fn from(iter: I) -> Self {
        SerIter(RefCell::new(iter))
    }
}

// Clippy fires false-positive
#[allow(clippy::while_let_on_iterator)]
impl<I, T> Serialize for SerIter<I>
where
    T: Serialize,
    I: Iterator<Item = T>,
{
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(None)?;
        let mut iter = self.0.borrow_mut();
        while let Some(item) = iter.next() {
            seq.serialize_element(&item)?;
        }
        seq.end()
    }
}
