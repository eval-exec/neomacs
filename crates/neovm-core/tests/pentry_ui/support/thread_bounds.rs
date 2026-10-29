//! Keep the first diagnostic in fixture code, without rust-src excerpts.
#![allow(dead_code)]
use std::marker::PhantomData;

pub struct ThreadTransfer<T>(PhantomData<T>);
impl<T: Send> ThreadTransfer<T> {
    pub fn check() {}
}

pub struct ThreadSharing<T>(PhantomData<T>);
impl<T: Sync> ThreadSharing<T> {
    pub fn check() {}
}
