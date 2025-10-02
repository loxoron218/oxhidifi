use std::{
    borrow::Borrow,
    collections::{HashMap, VecDeque},
    hash::Hash,
    num::NonZeroUsize,
};

/// A Least Recently Used (LRU) cache implementation.
///
/// This cache maintains a fixed-size collection of key-value pairs, automatically
/// evicting the least recently accessed items when the cache reaches its capacity.
///
/// The cache uses a combination of a HashMap for O(1) lookups and a VecDeque to
/// track the order of usage, ensuring that the least recently used items are
/// removed first when the cache is full.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// # use crate::src::utils::image::cache::lru::LruCache;
///
/// let capacity = NonZeroUsize::new(3).unwrap();
/// let mut cache = LruCache::new(capacity);
///
/// cache.put("key1", "value1");
/// cache.put("key2", "value2");
///
/// assert_eq!(cache.get(&"key1"), Some(&"value1"));
/// assert_eq!(cache.get(&"key2"), Some(&"value2"));
/// ```
pub struct LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// The underlying HashMap storing key-value pairs for O(1) lookups.
    map: HashMap<K, V>,

    /// A VecDeque songing the order of key accesses, with the most recently
    /// used keys at the front and least recently used at the back.
    order: VecDeque<K>,

    /// The maximum number of entries the cache can hold.
    capacity: NonZeroUsize,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Creates a new LRU cache with the specified capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The maximum number of entries the cache can hold.
    ///   Must be a non-zero value.
    ///
    /// # Returns
    ///
    /// A new `LruCache` instance with the specified capacity.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity.get()),
            order: VecDeque::with_capacity(capacity.get()),
            capacity,
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the key already exists in the cache, its value is updated and the key
    /// is moved to the front of the usage order. If the key does not exist and
    /// inserting it would exceed the cache capacity, the least recently used
    /// entry is automatically evicted.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to insert or update.
    /// * `value` - The value associated with the key.
    ///
    /// # Returns
    ///
    /// * `Some(V)` - If the key already existed, the old value is returned.
    /// * `None` - If the key did not exist before.
    pub fn put(&mut self, key: K, value: V) -> Option<V> {
        if let Some(old_value) = self.map.insert(key.clone(), value) {
            // Key already existed, update its position in the order
            if let Some(index) = self.order.iter().position(|k| k == &key) {
                self.order.remove(index);
            }
            self.order.push_front(key);
            Some(old_value)
        } else {
            // New key, add it to the front of the order
            self.order.push_front(key);

            // If we've exceeded capacity, remove the least recently used entry
            if self.order.len() > self.capacity.get()
                && let Some(key_to_remove) = self.order.pop_back()
            {
                self.map.remove(&key_to_remove);
            }
            None
        }
    }

    /// Retrieves a reference to the value associated with the given key.
    ///
    /// If the key exists, it is moved to the front of the usage order to mark
    /// it as recently used. If the key does not exist, `None` is returned.
    ///
    /// # Arguments
    ///
    /// * `key` - Reference to the key to look up.
    ///
    /// # Returns
    ///
    /// * `Some(&V)` - Reference to the value if the key exists.
    /// * `None` - If the key does not exist in the cache.
    pub fn get<Q: ?Sized>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if self.map.get(key).is_some() {
            // Key exists, update its position in the order
            if let Some(index) = self.order.iter().position(|k| k.borrow() == key)
                && let Some(k) = self.order.remove(index)
            {
                self.order.push_front(k);
            }
            self.map.get(key)
        } else {
            None
        }
    }

    /// Removes and returns the value associated with the given key.
    ///
    /// If the key exists, both the key-value pair and its position in the
    /// usage order are removed from the cache. If the key does not exist,
    /// `None` is returned.
    ///
    /// # Arguments
    ///
    /// * `key` - Reference to the key to remove.
    ///
    /// # Returns
    ///
    /// * `Some(V)` - The value if the key existed and was removed.
    /// * `None` - If the key did not exist in the cache.
    pub fn pop<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if let Some(value) = self.map.remove(key) {
            // Remove the key from the order songing as well
            if let Some(index) = self.order.iter().position(|k| k.borrow() == key) {
                self.order.remove(index);
            }
            Some(value)
        } else {
            None
        }
    }

    /// Removes and returns the least recently used entry from the cache.
    ///
    /// This method removes the entry at the back of the usage order, which
    /// represents the least recently accessed item in the cache.
    ///
    /// # Returns
    ///
    /// * `Some((K, V))` - The key-value pair of the least recently used entry.
    /// * `None` - If the cache is empty.
    pub fn pop_lru(&mut self) -> Option<(K, V)> {
        if let Some(key) = self.order.pop_back()
            && let Some(value) = self.map.remove(&key)
        {
            return Some((key, value));
        }
        None
    }

    /// Checks if the cache contains the specified key.
    ///
    /// This method checks for key existence without modifying the usage order.
    ///
    /// # Arguments
    ///
    /// * `key` - Reference to the key to check for.
    ///
    /// # Returns
    ///
    /// * `true` - If the key exists in the cache.
    /// * `false` - If the key does not exist.
    pub fn contains<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.contains_key(key)
    }

    /// Checks if the cache is empty.
    ///
    /// # Returns
    ///
    /// * `true` - If the cache contains no entries.
    /// * `false` - If the cache contains at least one entry.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the capacity of the cache.
    ///
    /// # Returns
    ///
    /// The maximum number of entries the cache can hold.
    pub fn cap(&self) -> NonZeroUsize {
        self.capacity
    }
}
