//! Send/Sync test bounds. The goldens of their negatives name $RUST/alloc (Rc), so they need rust-src.
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
