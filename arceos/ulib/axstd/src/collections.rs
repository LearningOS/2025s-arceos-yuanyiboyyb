#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};

use axhal::misc::random;

use core::hash::{Hash, Hasher,SipHasher};

use core::mem;

const INITIAL_CAPACITY: usize =  150_000;

pub struct HashMap<K, V>
where
    K: Eq + Hash,
{
    buckets: Vec<Option<(K, V)>>,
    capacity: usize,
    size: usize,
}

impl<K: Eq + Hash + Clone, V> HashMap<K, V> {
    pub fn new() -> Self {
        let capacity = INITIAL_CAPACITY;
        let mut buckets = Vec::with_capacity(capacity);
        for i in 0..capacity{
            buckets.push(None);
        }
        HashMap {
            buckets,
            capacity,
            size: 0,
        }
    }

    fn hash(&self, key: &K) -> usize {
        let mut hasher = SipHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.capacity
    }

    pub fn insert(&mut self, key: K, value: V) {
        let index = self.hash(&key);
        for i in 0..INITIAL_CAPACITY{
            match &self.buckets[index+i]{
                Some((k, _)) if *k == key => {
                    self.buckets[index] = Some((key, value));
                    break;
                }
                None =>{
                    self.buckets[index] = Some((key, value));
                    self.size += 1;
                    break;
                }
                _ =>{}
            } 
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let index = self.hash(key);
        let mut answer:Option<&V> = None;
        for i in 0..INITIAL_CAPACITY{
             match &self.buckets[index+i]{
                Some((k, v)) if k == key => {
                    answer = Some(v);
                    break;
                }
                None=>{
                    break;
                }
                _=>{}
             }
        }
        answer
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = self.hash(key);
        let mut answer = None;
        for i in 0..INITIAL_CAPACITY{
            match &self.buckets[index+i] {
                Some((k, v)) if k == key => {
                    let removed = self.buckets[index].take();
                    self.size -= 1;
                    answer = removed.map(|(_, v)| v);
                    break;
                }
                None =>{
                    break;
                }
                _ => {}
            }
        }
        answer
    }


}


pub struct HashMapIter<'a, K, V>
where
    K: Eq + Hash,
{
    map: &'a HashMap<K, V>,
    current_index: usize,
}

impl<K: Eq + Hash + Clone, V> HashMap<K, V> {
    // 创建迭代器的方法
    pub fn iter(&self) -> HashMapIter<'_, K, V> {
        HashMapIter {
            map: self,
            current_index: 0,
        }
    }
}

impl<'a, K: Eq + Hash, V> Iterator for HashMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.map.capacity {
            if let Some((ref key, ref value)) = self.map.buckets[self.current_index] {
                self.current_index += 1;
                return Some((key, value));
            }
            self.current_index += 1;
        }
        None
    }
}
