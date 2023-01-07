use cc_traits::{Collection, Insert, Iter, Len};
use rand::prelude::*;

/// Something that can be the backing storage for a `CommonCache`.
pub trait Backend: Collection + Len + Insert {
    /// Create a new instance of this collection with a single element.
    ///
    /// Note that this trait provides no way to make the collection empty, so the collection
    /// doesn't need to support that trivial case.
    fn singleton(elem: Self::Item) -> Self;

    /// Remove a **random** element from the collection given a random index.
    ///
    /// This function should remove a randomly selected element from the collection. Given to the function is a
    /// randomly generated integer `i`, in the range [0..self.len()]. It is however not mandatory
    /// to make any use of that index at all, the implementor can just ignore it and provide
    /// her/his own random generator instead. The index is computed anyway, so there is no extra
    /// overhead to pass it to this function in case it might be useful.
    ///
    /// It is **important** that this is really **psuedo random** for the cache to behave as
    /// expected. Just `hash_map.keys().next()` is not good enough.
    ///
    /// Will not be called when `self.len() == 0`. That should be obvious to the reader since
    /// there is no valid value for `i` in that case.
    fn remove_random(&mut self, i: usize) -> Self::Item;
}

/// A collection which keeps and promotes the most recently and commonly used items.
#[derive(Debug, Clone)]
pub struct CommonCache<T: Backend, R: Rng = StdRng> {
    /// The base for the exponentially growing size of buckets.
    base: usize,
    /// All active buckets in the cache
    ///
    /// These will at most have size [1, base, base^2, base^3, ...] and the last will not be empty.
    buckets: Vec<Bucket<T>>,
    /// A random number generator.
    rng: R,
}

#[derive(Debug, Clone)]
struct Bucket<T: Backend> {
    items: T,
    /// An instance of a uniform distribution to generate random numbers in the range [0..base^n],
    /// where n is the index of this bucket.
    rand_range: rand::distributions::Uniform<usize>,
}

impl<T: Backend, R: Rng> CommonCache<T, R> {
    /// Create a new `CommonCache` with a specific base and `Rng` generated from some entropy.
    pub fn new(base: usize) -> Self
    where
        R: SeedableRng,
    {
        Self::new_with_rng(base, R::from_entropy())
    }

    /// Create a new `CommonCache` with a given random generator. This can be useful if you have a
    /// psuedo random generator and want deterministic and reproduceable behaviour.
    pub fn new_with_rng(base: usize, rng: R) -> Self {
        Self {
            base,
            rng,
            buckets: Vec::new(),
        }
    }

    /// Insert a value into the cache.
    ///
    /// The value will be inserted at the second lowest bucket. So if `self.base == 2`, then it
    /// will be inserted in the second quarter in the cache.
    pub fn insert(&mut self, item: T::Item) {
        self.insert_at_level(item, self.buckets.len() - 2)
    }

    /// Insert an item at a specific level in the cache and possibly push an item to lower levels.
    ///
    /// This is the core function of the algorithm. It will, with probability k/n, (where n is the
    /// maximum number of items at the level and k is the actual number of items), remove an item
    /// from the level and insert it on the level below. This will be repeated for all lower
    /// levels. If an item is selected at the lowest level, a new lowest level will be created.
    ///
    /// The function will of course also insert the given item at the given level.
    fn insert_at_level(&mut self, item: T::Item, level: usize) {
        // Loop through all levels from the lowest to the current (`level`).c For each level,
        // randomly decide whether to move one item down to the level below. The fuller a level is,
        // the higher probability it is that an item will be moved down from that level.
        for level in (level..self.buckets.len()).rev() {
            let bucket = &mut self.buckets[level];
            // Generate an integer in the range of the total capacity of the level.
            let i = bucket.rand_range.sample(&mut self.rng);
            // If `i` is in the range of actual items on this level, move one item down.
            if i < bucket.items.len() {
                let move_down_item = bucket.items.remove_random(i);

                if level != self.buckets.len() - 1 {
                    // Insert the item on the level below.
                    self.buckets[level + 1].items.insert(move_down_item);
                } else {
                    // This was the lowest level. So let's create a new one.
                    let new_level_size = self
                        .base
                        .checked_pow((level + 1).try_into().unwrap_or(u32::MAX))
                        .unwrap_or(usize::MAX);
                    self.buckets.push(Bucket {
                        items: T::singleton(move_down_item),
                        rand_range: (0..new_level_size).into(),
                    });
                }
            }
        }
        // Finally, add the item to the desired level.
        self.buckets[level].items.insert(item);
    }

    /// Iterate over the elements in the cache so that all items on any level will come before any
    /// item on any lower level.
    pub fn iter(&self) -> impl Iterator<Item = T::ItemRef<'_>> + '_
    where
        T: Iter,
    {
        self.buckets.iter().flat_map(|x| x.items.iter())
    }
}
