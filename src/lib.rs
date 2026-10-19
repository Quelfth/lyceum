use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{
        RwLock,
        RwLockReadGuard,
        RwLockWriteGuard,
        atomic::{AtomicUsize, Ordering::*}
    }
};

use concurrent_queue::ConcurrentQueue;

struct Value<T> {
    generation: AtomicUsize,
    lock: RwLock<Option<T>>,
}

impl<T> Default for Value<T> {
    fn default() -> Self {
        Self { generation: Default::default(), lock: Default::default() }
    }
}

pub struct Lyceum<K, T> {
    data: boxcar::Vec<Value<T>>,
    realloc: ConcurrentQueue<usize>,
    _phantom: PhantomData<K>,
}

pub struct Key<K> {
    index: usize,
    generation: usize,
    _phantom: PhantomData<K>,
}

impl<K> Clone for Key<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Key<K> {}

pub struct LyceumGuard<'a, T>(RwLockReadGuard<'a, Option<T>>);

impl<'a, T> Deref for LyceumGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct LyceumGuardMut<'a, T>(RwLockWriteGuard<'a, Option<T>>);

impl<'a, T> DerefMut for LyceumGuardMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

impl<'a, T> Deref for LyceumGuardMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl<K, T> Lyceum<K, T> {
    pub fn insert(&self, value: T) -> Key<K> {
        let index = if let Ok(index) = self.realloc.pop() {
            index
        } else {
            self.data.push(Value::default())
        };

        let slot = &self.data[index];

        let generation = slot.generation.load(Acquire);
        *slot.lock.write().unwrap() = Some(value);

        Key {
            index,
            generation,
            _phantom: PhantomData,
        }
    }

    fn access(&self, key: Key<K>) -> Option<&Value<T>> {
        let slot = &self.data.get(key.index)?;
        if slot.generation.load(Relaxed) != key.generation { None? }
        Some(slot)
    }

    pub fn get(&self, key: Key<K>) -> Option<LyceumGuard<'_, T>> {
        Some(LyceumGuard(self.access(key)?.lock.read().unwrap()))
    }

    pub fn get_mut(&self, key: Key<K>) -> Option<LyceumGuardMut<'_, T>> {
        Some(LyceumGuardMut(self.access(key)?.lock.write().unwrap()))
    }

    pub fn get_copied(&self, key: Key<K>) -> Option<T> where T: Copy {
        Some(*self.get(key)?)
    }

    pub fn get_cloned(&self, key: Key<K>) -> Option<T> where T: Clone {
        Some(self.get(key)?.clone())
    }

    pub fn take(&self, key: Key<K>) -> Option<T> {
        let slot = self.access(key)?;
        let value = slot.lock.write().unwrap().take()?;
        self.realloc.push(key.index).unwrap();
        slot.generation.fetch_add(1, Release);

        Some(value)
    }

    pub fn remove(&self, key: Key<K>) {
        self.take(key);
    }
}
