use std::{collections::HashMap, hash::Hash, sync::Arc};

use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct SharedMap<K, V>(Arc<Mutex<HashMap<K, V>>>);

impl<K, V> Default for SharedMap<K, V> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(HashMap::default())))
    }
}

impl<K, V> SharedMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub async fn insert(&self, key: K, value: V) {
        self.0.lock().await.insert(key, value);
    }

    pub async fn remove(&self, key: &K) -> Option<V> {
        self.0.lock().await.remove(key)
    }

    pub async fn all(&self) -> HashMap<K, V> {
        self.0.lock().await.clone()
    }
}
