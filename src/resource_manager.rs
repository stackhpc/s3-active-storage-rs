//! Resource management

use crate::error::ActiveStorageError;

use tokio::sync::{Semaphore, SemaphorePermit};

/// [crate::resource_manager::ResourceManager] provides a simple way to allocate various resources
/// to tasks. Resource management is performed using a Tokio Semaphore for each type of resource.
#[derive(Debug)]
pub struct ResourceManager {
    /// Optional semaphore for S3 connections.
    s3_connections: Option<Semaphore>,

    /// Optional semaphore for memory (bytes).
    memory: Option<Semaphore>,

    /// Optional total memory pool in bytes.
    total_memory: Option<usize>,

    /// Optional semaphore for tasks.
    tasks: Option<Semaphore>,
}

impl ResourceManager {
    /// Returns a new ResourceManager object.
    pub fn new(
        s3_connection_limit: Option<usize>,
        memory_limit: Option<usize>,
        task_limit: Option<usize>,
    ) -> Self {
        Self {
            s3_connections: s3_connection_limit.map(Semaphore::new),
            memory: memory_limit.map(Semaphore::new),
            total_memory: memory_limit,
            tasks: task_limit.map(Semaphore::new),
        }
    }

    /// Acquire an S3 connection resource.
    pub async fn s3_connection(&self) -> Result<Option<SemaphorePermit>, ActiveStorageError> {
        optional_acquire(&self.s3_connections, 1).await
    }

    /// Acquire memory resource.
    pub async fn memory(
        &self,
        bytes: usize,
    ) -> Result<Option<SemaphorePermit>, ActiveStorageError> {
        if let Some(total_memory) = self.total_memory {
            if bytes > total_memory {
                return Err(ActiveStorageError::InsufficientMemory {
                    requested: bytes,
                    total: total_memory,
                });
            };
        };
        optional_acquire(&self.memory, bytes).await
    }

    /// Acquire a task resource.
    pub async fn task(&self) -> Result<Option<SemaphorePermit>, ActiveStorageError> {
        optional_acquire(&self.tasks, 1).await
    }
}

/// Acquire permits on an optional Semaphore, if present.
async fn optional_acquire(
    sem: &Option<Semaphore>,
    n: usize,
) -> Result<Option<SemaphorePermit>, ActiveStorageError> {
    let n = n.try_into()?;
    if let Some(sem) = sem {
        sem.acquire_many(n)
            .await
            .map(Some)
            .map_err(|err| err.into())
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::TryAcquireError;

    #[tokio::test]
    async fn no_resource_management() {
        let rm = ResourceManager::new(None, None, None);
        assert!(rm.s3_connections.is_none());
        assert!(rm.memory.is_none());
        assert!(rm.tasks.is_none());
        let _c = rm.s3_connection().await.unwrap();
        let _m = rm.memory(1).await.unwrap();
        let _t = rm.task().await.unwrap();
        assert!(_c.is_none());
        assert!(_m.is_none());
        assert!(_t.is_none());
    }

    #[tokio::test]
    async fn full_resource_management() {
        let rm = ResourceManager::new(Some(1), Some(1), Some(1));
        assert!(rm.s3_connections.is_some());
        assert!(rm.memory.is_some());
        assert!(rm.tasks.is_some());
        let _c = rm.s3_connection().await.unwrap();
        let _m = rm.memory(1).await.unwrap();
        let _t = rm.task().await.unwrap();
        assert!(_c.is_some());
        assert!(_m.is_some());
        assert!(_t.is_some());
        // Check that there are no more resources (without blocking).
        assert_eq!(
            rm.s3_connections.as_ref().unwrap().try_acquire().err(),
            Some(TryAcquireError::NoPermits)
        );
        assert_eq!(
            rm.memory.as_ref().unwrap().try_acquire().err(),
            Some(TryAcquireError::NoPermits)
        );
        assert_eq!(
            rm.tasks.as_ref().unwrap().try_acquire().err(),
            Some(TryAcquireError::NoPermits)
        );
    }
}
