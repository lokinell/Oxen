use crate::models::TokenRecord;
use rocksdb::{DB, Options, WriteBatch};
use std::path::Path;
use std::sync::Arc;

pub struct TokenStore {
    db: Arc<DB>,
}

impl TokenStore {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Atomically insert a token record. If a record with the same name already exists,
    /// the old token: key is removed before writing the new pair.
    pub fn insert(
        &self,
        record: &TokenRecord,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let name_key = format!("name:{}", record.name);
        let token_key = format!("token:{}", record.token);
        let value = serde_json::to_vec(record)?;

        let mut batch = WriteBatch::default();

        // Remove the old token entry if this name already maps to one.
        if let Some(old_bytes) = self.db.get(name_key.as_bytes())? {
            let old_key = format!("token:{}", String::from_utf8_lossy(&old_bytes));
            batch.delete(old_key.as_bytes());
        }

        batch.put(token_key.as_bytes(), &value);
        batch.put(name_key.as_bytes(), record.token.as_bytes());
        self.db.write(batch)?;
        Ok(())
    }

    pub fn get_by_token(
        &self,
        token: &str,
    ) -> Result<Option<TokenRecord>, Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("token:{}", token);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Atomically delete both index keys for a token identified by name.
    pub fn delete_by_name(
        &self,
        name: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let name_key = format!("name:{}", name);
        match self.db.get(name_key.as_bytes())? {
            Some(token_bytes) => {
                let token = String::from_utf8(token_bytes.to_vec())?;
                let mut batch = WriteBatch::default();
                batch.delete(format!("token:{token}").as_bytes());
                batch.delete(name_key.as_bytes());
                self.db.write(batch)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn list_all(&self) -> Result<Vec<TokenRecord>, Box<dyn std::error::Error + Send + Sync>> {
        let prefix = b"token:";
        let mut records = Vec::new();
        for item in self.db.prefix_iterator(prefix) {
            let (key, value) = item?;
            if !key.starts_with(prefix) {
                break;
            }
            match serde_json::from_slice::<TokenRecord>(&value) {
                Ok(record) => records.push(record),
                Err(e) => log::error!(
                    "failed to deserialize token record at key={}: {e}",
                    String::from_utf8_lossy(&key[prefix.len()..])
                ),
            }
        }
        Ok(records)
    }
}
