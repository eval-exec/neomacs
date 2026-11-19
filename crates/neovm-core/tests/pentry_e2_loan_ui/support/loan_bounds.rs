//! Keep the first diagnostic in fixture code, without rust-src excerpts.
#![allow(dead_code)]
use std::marker::PhantomData;

pub struct RequireClone<T>(PhantomData<T>);
impl<T: Clone> RequireClone<T> {
    pub fn check() {}
}

pub struct RequireCopy<T>(PhantomData<T>);
impl<T: Copy> RequireCopy<T> {
    pub fn check() {}
}

pub struct RequireSync<T>(PhantomData<T>);
impl<T: Sync> RequireSync<T> {
    pub fn check() {}
}

pub struct RequireSend<T>(PhantomData<T>);
impl<T: Send> RequireSend<T> {
    pub fn check() {}
}
