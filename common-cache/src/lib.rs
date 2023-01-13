//! This is a small, fast on average, and quite intuative implementation of a cache-like structure,
//! which keeps and promotes the most commonly and most recently used items.
//!
//! ## General characteristics
//!
//! The structure roughly have the following properties:
//! - Elements that are used more often and more recently are placed higher in the cache.
//! - Items that haven't be used for a while might be discarded from the cache.
//! - If many elements are used, the cache will grow in size. And if only a few elements are used,
//! the cache will slowly discard the unused items and shrink in size.
//! - The cache uses some randomness under the hood to select what items to move down and discard.
//! But don't worry, you can provide your own RNG if you want reproducability.
//! - Most operations, like insert, remove and lookup, runs in logarithmic time.
//!
//! ## Details
//!
//! The structure basicly works as follows:
//! - The cache is devided into levels which grows exponentially. Think of it as a pyramid: At the
//! top there is a level with 2^0 = 1 items, below there is a level with capacity for 2^1 = 2 items, then
//! with 2^2 = 4, then with 2^3 = 8, and so on. That "2" is not magic and any integral base >1 can
//! be used, it will be refered to as the variable "base".
//! - So the size of level with index n is base^n.  However most levels will usually just be partly
//! filled.
//! - When initializing the cache, all levels are empty.
//! - When inserting into the cache, the following steps are performed:
//!     1. If the item is already in the cache at level l, remove it and set the insertion level to
//!        be the level above l (or l again if it was the top). Then goto step three.
//!     2. If the item was not in the cache, set the insertion level to be the second lowest level.
//!        If we have base=2, this means that it will be in the second quarter.
//!     3. Loop through all levels from the lowest (non-empty) level, up to and including the
//!        insertion level. For each level do:
//!         1. Generate a random integer i in the range [0..2^l], where l is the index of the
//!         level (with the top having index 0). That range corresponds to the size of the level.
//!         2. If i is less than the number of actual elements stored at that
//!         level, move the ithe item on this level to the level below.
//!     4. Finally insert the item at the right level.
//! - When one element is accessed with a get operation, it is removed from its current level and
//! inserted on the level above, following the same algorithm as described above with the only
//! difference that if an item is moved down from the lowest level, a new level is not created and
//! that item gets discarded.
//! - When iterating over the cache, all levels are visited in order. So no element on any level will
//! come after any element on a level below.
use core::borrow::Borrow;
use core::hash::Hash;
use core::marker::PhantomData;
use indexmap::IndexMap;
use rand::prelude::*;
use replace_with::replace_with_or_abort;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A collection which keeps and promotes the most recently and commonly used items.
///
/// See the module level documentation for details.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CommonCache<K, V, R: Rng = StdRng> {
    /// The base for the exponentially growing size of levels.
    base: usize,
    /// All active levels in the cache
    ///
    /// These will at most have size [1, base, base^2, base^3, ...] and the last will not be empty.
    #[cfg_attr(
        feature = "serde",
        serde(bound(
            deserialize = "K: Deserialize<'de> + Eq + Hash, V: Deserialize<'de>",
            serialize = "K: Serialize + Eq + Hash, V: Serialize",
        ))
    )]
    levels: Vec<Level<K, V>>,
    /// A random number generator.
    #[serde(skip, default = "SeedableRng::from_entropy", bound = "R: SeedableRng")]
    rng: R,

    /// An upper bound of the number of elements in the cache. Might be set to `usize::MAX`.
    max_size: usize,

    /// A counter that increments every time elements are moved between levels in the cache.
    ///
    /// Used to prevent that a `Index` might randomly be invalidated because some random element
    /// has been moved to another level. Instead, an `Index` is invalid if the generation on the
    /// index and the cache differs.
    generation: u64,
}

/// A level in the cache.
#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(bound(
        deserialize = "K: Deserialize<'de> + Eq + Hash, V: Deserialize<'de>",
        serialize = "K: Serialize + Eq + Hash, V: Serialize",
    ))
)]
struct Level<K, V> {
    items: IndexMap<K, V>,
    /// An instance of a uniform distribution to generate random numbers in the range [0..base^n],
    /// where n is the index of this level.
    rand_range: rand::distributions::Uniform<usize>,
}

impl<K, V> CommonCache<K, V> {
    /// Create a new `CommonCache` with a specific base and `Rng` generated from some entropy.
    ///
    /// Takes a base which must be >1 and optionally a max_size which must be >=2.
    pub fn new(base: usize, max_size: Option<usize>) -> Self {
        Self::new_with_rng(base, max_size, StdRng::from_entropy())
    }

    /// Get the currently configured max size for the cache.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Set the max size. Note that if this might cause many elements to be removed.
    ///
    /// PRE: max_size >= 2
    ///
    /// Runs in linear time if max_size < self.size(), constant time otherwise.
    ///
    /// If the new max_size is less than the previous, all indexes to this cache will be
    /// invalidated. This is because some elements might be removed randomly from the cache and we
    /// don't want some index lookup to randomly work/fail.
    pub fn set_max_size(&mut self, max_size: usize) {
        assert!(
            max_size >= 2,
            "max_size must be >=2 in CommonCache::set_max_size()"
        );
        if max_size >= self.max_size {
            self.max_size = max_size;
            return;
        }
        let mut sum = 0;
        for (i, level) in self.levels.iter_mut().enumerate() {
            if sum == max_size {
                self.levels.truncate(i);
                break;
            }
            sum += level.items.len();
            if sum > max_size {
                for _ in max_size..sum {
                    let to_remove = self.rng.gen_range(0..level.items.len());
                    level.items.swap_remove_index(to_remove);
                }
                self.levels.truncate(i + 1);
                break;
            }
        }

        // Some random elements might have been removed so let's increase the generation to
        // invalidate any indexes to the cache.
        self.generation += 1;
    }

    /// Clear the cache.
    pub fn clear(&mut self) {
        self.levels.clear();
        self.generation += 1;
    }
}

impl<K, V, R: Rng> CommonCache<K, V, R> {
    /// Create a new `CommonCache` with a given random generator. This can be useful if you have a
    /// psuedo random generator and want deterministic and reproduceable behaviour.
    ///
    /// Also takes in a base which must be >1 and optionally a max_size which must be >=2.
    pub fn new_with_rng(base: usize, max_size: Option<usize>, rng: R) -> Self {
        let max_size = max_size.unwrap_or(usize::MAX);
        assert!(max_size >= 2, "max_size in CommonCache must be >= 2");
        assert!(base >= 2, "base in CommonCache must be >=2.");
        Self {
            base,
            rng,
            levels: Vec::new(),
            max_size,
            generation: 0,
        }
    }

    /// Get the number of elements in the cache.
    ///
    /// Runs in O(log\[base](n)) time, since the len of all levels must be summed up.
    pub fn size(&self) -> usize {
        self.levels.iter().map(|x| x.items.len()).sum()
    }
}

impl<K, V, R> CommonCache<K, V, R>
where
    K: Eq + Hash,
    R: Rng,
{
    /// Insert a value into the cache.
    ///
    /// If the value is new, it will be inserted at the second lowest level. So if `self.base == 2`, then it
    /// will be inserted in the second quarter in the cache.
    ///
    /// If the value exists in the cache already though, it will be updated with the new key and
    /// value and be moved to one level above its previous position.
    ///
    /// **IMPORTANT: All `Index` to elements in this cache will be invalidated** This is because
    /// some random elements will be moved between levels in the cache, and we don't want indexes
    /// to be invalidated randomly. It might cause some erroneous tests to pass undeterministicly.
    ///
    /// # Returns
    ///
    /// Returns the entry for the newly inserted item.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use common_cache::CommonCache;
    /// use assert_matches::assert_matches;
    ///
    /// let mut cache = CommonCache::new(2, None);
    /// let mut entry = cache.insert(4, "Hello");
    /// assert_matches!(*entry.get_value(), "Hello");
    /// ```
    pub fn insert(&mut self, key: K, value: V) -> Entry<'_, K, V, R> {
        // Check if the item is already in the cache.
        let insert_level = if let Some(entry) = self.entry(&key) {
            let level = entry.level;
            let _old_item = entry.remove();
            // Insert the item at the level above.
            level.saturating_sub(1)
        } else {
            // If the item is new, insert it in the second lowest level.
            self.levels.len().saturating_sub(2)
        };
        self.insert_at_level::<true>(key, value, insert_level)
    }

    /// Insert an item at a specific level in the cache and possibly push an item to lower levels.
    ///
    /// This is the core function of the algorithm. It will, with probability k/n, (where n is the
    /// maximum number of items at the level and k is the actual number of items), remove an item
    /// from the level and insert it on the level below. This will be repeated for all lower
    /// levels. If an item is selected at the lowest level, a new lowest level will be created.
    ///
    /// The function will of course also insert the given item at the given level.
    ///
    /// `self.generation` is increased, so all `Index`es to this cache are invalidated.
    fn insert_at_level<const CREATE_NEW_LEVEL_IF_NEEDED: bool>(
        &mut self,
        key: K,
        value: V,
        level: usize,
    ) -> Entry<'_, K, V, R> {
        // Let's increment the generation immediately so we don't forget it.
        self.generation += 1;

        if self.size() == self.max_size {
            // If the max size has been reached.
            let last_level_items = &mut self.levels.last_mut().unwrap().items;
            let to_remove = self.rng.gen_range(0..last_level_items.len());
            last_level_items.swap_remove_index(to_remove);
            if last_level_items.is_empty() {
                self.levels.pop();
            }
        }

        if self.levels.is_empty() {
            // If there are no levels, add one.
            self.levels.push(Level {
                items: IndexMap::with_capacity(1),
                rand_range: (0..1).into(),
            });
        }

        // Loop through all levels from the lowest to the current (`level`).c For each level,
        // randomly decide whether to move one item down to the level below. The fuller a level is,
        // the higher probability it is that an item will be moved down from that level.
        for level in (level..self.levels.len()).rev() {
            let current_level = &mut self.levels[level];
            // Generate an integer in the range of the total capacity of the level.
            let i = current_level.rand_range.sample(&mut self.rng);
            if let Some(move_down_item) = current_level.items.swap_remove_index(i) {
                if level != self.levels.len() - 1 {
                    // Insert the item on the level below.
                    self.levels[level + 1]
                        .items
                        .insert(move_down_item.0, move_down_item.1);
                } else if CREATE_NEW_LEVEL_IF_NEEDED {
                    // This was the lowest level. So let's create a new one.
                    let new_level_size = self
                        .base
                        .checked_pow((level + 1).try_into().unwrap_or(u32::MAX))
                        .unwrap_or(usize::MAX);
                    self.levels.push(Level {
                        items: IndexMap::from([move_down_item]),
                        rand_range: (0..new_level_size).into(),
                    });
                }
            }
        }
        // Finally, add the item to the desired level.
        let (idx, None) = self.levels[level].items.insert_full(key, value) else {
            unreachable!()
        };
        Entry {
            cache: self,
            level,
            idx,
        }
    }

    /// Get a handle to an entry in the cache.
    ///
    /// Runs in `O(log[base](n))` time.
    pub fn entry<Q>(&mut self, key: &Q) -> Option<Entry<'_, K, V, R>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        if let Some((level, idx)) = self
            .levels
            .iter_mut()
            .enumerate()
            .filter_map(|(i, x)| x.items.get_index_of(key).map(|x| (i, x)))
            .next()
        {
            Some(Entry {
                cache: self,
                level,
                idx,
            })
        } else {
            None
        }
    }

    /// Iterate over the elements in the cache so that all items on any level will come before any
    /// item on any lower level.
    ///
    /// This does not alter the cache in any way. So no items are promoted to higher levels in the
    /// cache when iterated over.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&'_ K, &'_ V)> + '_ {
        self.levels.iter().flat_map(|x| x.items.iter())
    }

    /// Iterate over mutable references to the elements in the cache. All items on any level will come before any
    /// item on any lower level.
    ///
    /// This does not alter the structure of the cache. So no items are promoted to higher levels in the
    /// cache when iterated over.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&'_ K, &'_ mut V)> {
        self.levels.iter_mut().flat_map(|x| x.items.iter_mut())
    }

    /// Find the first item in the cache matching a predicate.
    ///
    /// The advantage of using this method over `self.iter().find()` is that you get an `Entry`
    /// from this which can be used to promote or remove the item with.
    pub fn find_first(
        &mut self,
        mut pred: impl FnMut(&K, &V) -> bool,
    ) -> Option<Entry<'_, K, V, R>> {
        if let Some((level, (idx, _))) = self
            .levels
            .iter()
            .enumerate()
            .flat_map(|(i, level)| level.items.iter().enumerate().map(move |x| (i, x)))
            .filter(|(_, (_, (key, val)))| pred(key, val))
            .next()
        {
            Some(Entry {
                cache: self,
                level,
                idx,
            })
        } else {
            None
        }
    }
}

/// A reference to an occupied entry in the cache.
#[derive(Debug)]
pub struct Entry<'a, K, V, R: Rng = StdRng> {
    /// A reference to the entire cache.
    cache: &'a mut CommonCache<K, V, R>,
    /// The index of the level for the entry.
    level: usize,
    /// The index for the entry in the level.
    idx: usize,
}

impl<'a, K: Eq + Hash, V, R: Rng> Entry<'a, K, V, R> {
    /// Read the key and value at the entry without touching the rest of the cache. This operation
    /// will hence not be taken into account when considering which elements are most commonly
    /// used.
    pub fn peek_key_value(&self) -> (&K, &V) {
        self.cache.levels[self.level]
            .items
            .get_index(self.idx)
            .unwrap()
    }

    /// Silently read the key at this entry.
    pub fn peek_key(&self) -> &K {
        self.peek_key_value().0
    }

    /// Read the value at the entry without touching the rest of the cache. This operation
    /// will hence not be taken into account when considering which elements are most commonly
    /// used.
    pub fn peek_value(&self) -> &V {
        self.peek_key_value().1
    }

    /// Read the entry mutably without touching the rest of the cache. This operation will not
    /// be taken into account when considering which elements are most commonly used.
    pub fn peek_key_value_mut(&mut self) -> (&K, &mut V) {
        let (key, value) = self.cache.levels[self.level]
            .items
            .get_index_mut(self.idx)
            .unwrap();
        (&*key, value)
    }

    /// Read the value mutably without touching the rest of the cache. This operation will not
    /// be taken into account when considering which elements are most commonly used.
    pub fn peek_value_mut(&mut self) -> &mut V {
        self.peek_key_value_mut().1
    }

    /// Read the item at this entry and destroy the `Entry` struct. The item will still be in the
    /// cache but this allows us to get a reference with the full lifetime of this entry.
    pub fn peek_long(self) -> (&'a K, &'a mut V) {
        let (key, value) = self.cache.levels[self.level]
            .items
            .get_index_mut(self.idx)
            .unwrap();
        (&*key, value)
    }

    /// Get the key and value at this entry and promote this entry to a higher level in the cache.
    ///
    /// This function will promote this entry to a higher level in the cache and based on some
    /// probability move other items down in the cache.
    pub fn get_key_value(&mut self) -> (&K, &mut V) {
        replace_with_or_abort(self, |self_| {
            let curr_level = self_.level;
            let (index, cache) = self_.index_and_cache();
            let (key, value) = index.remove_from(cache);
            cache.insert_at_level::<false>(key, value, curr_level.saturating_sub(1))
        });
        self.peek_key_value_mut()
    }

    /// Get the value at this entry and promote this entry to a higher level in the cache.
    ///
    /// This function will promote this entry to a higher level in the cache and based on some
    /// probability move other items down in the cache.
    pub fn get_value(&mut self) -> &mut V {
        self.get_key_value().1
    }

    /// Get the key and value at this entry and promote this entry to a higher level in the cache.
    ///
    ///
    /// Unlike `Self::get_key_value`, this method consumes the `Entry` allowing for a longer
    /// lifetime of the returned reference. Note that the item will still remain in the cache
    /// though.
    ///
    /// This function will promote this entry to a higher level in the cache and based on some
    /// probability move other items down in the cache.
    pub fn get_long(mut self) -> (&'a K, &'a mut V) {
        self.get_key_value();
        self.peek_long()
    }

    /// Remove this entry from the cache. Leaving the rest of the cache intact.
    ///
    /// Runs in O(1) time.
    pub fn remove(self) -> (K, V) {
        let (index, cache) = self.index_and_cache();
        index.remove_from(cache)
    }

    /// Get an index for this entry.
    ///
    /// This is like the `Entry` without the reference to the cache. The `Index` will be
    /// invalidated though if the cache is altered in any way, including insertian of new elements
    /// or promotion of existing elements.
    pub fn index(self) -> Index<K, V, R> {
        self.index_and_cache().0
    }

    /// Split this entry to an index and the cache.
    ///
    /// The `Index` is like the `Entry` without the reference to the cache. The `Index` will be
    /// invalidated though if the cache is altered in any way, including insertian of new elements
    /// or promotion of existing elements.
    pub fn index_and_cache(self) -> (Index<K, V, R>, &'a mut CommonCache<K, V, R>) {
        (
            Index {
                level: self.level,
                idx: self.idx,
                generation: self.cache.generation,
                _key_ty: PhantomData,
                _val_ty: PhantomData,
                _rng_ty: PhantomData,
            },
            self.cache,
        )
    }
}

/// An index into a `CommonCache`.
///
/// This should be used when an `Entry` is not sufficient due to life time problems. Note however
/// that all indexes to a cache will be invalidated whenever the cache is altered in any way.
/// Including insertian of new elements and promotion of existing elements. This is because the
/// index is just a pointer to a specific level and index in that level of the cache. If some
/// element is inserted or promoted, a few other elements will **randomly** be moved down some
/// levels in the cache, causing their indexes to be invalid and potentially point to other items.
/// However, this happens randomly, so tests might not recognize such a bug, and it will not be
/// reproduceable.
///
/// The solution is to use an internal counter, (generation), which increments each time the cache
/// is altered. Each index has the generation of the cache when the index was created, and if the
/// index is used with a newer version of the cache it will be invalid.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Index<K, V, R: Rng = StdRng> {
    /// The index of the level for the item.
    level: usize,
    /// The index for the item whithin the level.
    idx: usize,
    /// The generation when this index was created.
    generation: u64,
    _key_ty: PhantomData<K>,
    _val_ty: PhantomData<V>,
    _rng_ty: PhantomData<R>,
}

impl<K: Eq + Hash, V, R: Rng> Index<K, V, R> {
    /// Assert that this index has the same generation as that of a cache. Panics otherwise.
    fn assert_generation(&self, cache: &CommonCache<K, V, R>) {
        assert_eq!(
            self.generation, cache.generation,
            "The generations of an `Index` and a `CommonCache` differs"
        );
    }

    /// Get an entry corresponding to this index.
    ///
    /// # Panics
    ///
    /// Panics if the generation of the cache and this index differs, I.E that something has been
    /// inserted or promoted in the cache since this index was created.
    ///
    /// Might also panic when trying to read the entry if the item corresponding to this index has
    /// been removed.
    pub fn entry(self, cache: &mut CommonCache<K, V, R>) -> Entry<'_, K, V, R> {
        self.assert_generation(cache);
        Entry {
            cache,
            level: self.level,
            idx: self.idx,
        }
    }

    /// Read the key and value at the index without touching the rest of the cache. This operation
    /// will hence not be taken into account when considering which elements are most commonly
    /// used.
    pub fn peek_key_value<'a>(&'a self, cache: &'a CommonCache<K, V, R>) -> (&'a K, &'a V) {
        self.assert_generation(cache);
        cache.levels[self.level].items.get_index(self.idx).unwrap()
    }

    /// Silently read the key at this index.
    pub fn peek_key<'a>(&'a self, cache: &'a CommonCache<K, V, R>) -> &'a K {
        self.peek_key_value(cache).0
    }

    /// Read the value at the index without touching the rest of the cache. This operation
    /// will hence not be taken into account when considering which elements are most commonly
    /// used.
    pub fn peek_value<'a>(&'a self, cache: &'a CommonCache<K, V, R>) -> &'a V {
        self.peek_key_value(cache).1
    }

    /// Read the item at this index mutably without touching the rest of the cache. This operation will not
    /// be taken into account when considering which elements are most commonly used.
    ///
    /// Note that this does not count as altering the cache so the index is still valid after this.
    pub fn peek_key_value_mut<'a>(
        &'a self,
        cache: &'a mut CommonCache<K, V, R>,
    ) -> (&'a K, &'a mut V) {
        self.assert_generation(cache);
        let (key, value) = cache.levels[self.level]
            .items
            .get_index_mut(self.idx)
            .unwrap();
        (&*key, value)
    }

    /// Read the value at this index mutably without touching the rest of the cache. This operation will not
    /// be taken into account when considering which elements are most commonly used.
    ///
    /// Note that this does not count as altering the cache so the index is still valid after this.
    pub fn peek_value_mut<'a>(&'a self, cache: &'a mut CommonCache<K, V, R>) -> &'a mut V {
        self.peek_key_value_mut(cache).1
    }

    /// Get the key and value at this index and promote the item to a higher level in the cache.
    ///
    /// This function will promote the item to a higher level in the cache and based on some
    /// probability move other items down in the cache.
    ///
    /// **The index will be invalidated after this operation.**
    pub fn get_key_value(self, cache: &mut CommonCache<K, V, R>) -> (&K, &mut V) {
        let curr_level = self.level;
        let (key, value) = self.remove_from(cache);
        cache
            .insert_at_level::<false>(key, value, curr_level.saturating_sub(1))
            .peek_long()
    }

    /// Get the value at this index and promote this index to a higher level in the cache.
    ///
    /// This function will promote this index to a higher level in the cache and based on some
    /// probability move other items down in the cache.
    pub fn get_value(self, cache: &mut CommonCache<K, V, R>) -> &mut V {
        self.get_key_value(cache).1
    }

    /// Remove the item at this index from the cache.
    fn remove_from(self, cache: &mut CommonCache<K, V, R>) -> (K, V) {
        self.assert_generation(cache);
        let level_items = &mut cache.levels[self.level].items;
        let (key, value) = level_items.swap_remove_index(self.idx).unwrap();
        if level_items.is_empty() && self.level == cache.levels.len() - 1 {
            // If the last level became empty, we shall remove it.
            cache.levels.pop();
        }
        (key, value)
    }
}
