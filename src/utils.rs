use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
