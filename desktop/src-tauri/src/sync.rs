//! Locking that survives a poisoned mutex — the shell's copy of the rule the
//! platform's crates hold in `bisa_core::sync` (the shell depends on none of
//! them, on purpose).
//!
//! A `std::sync::Mutex` is poisoned when a thread panics while holding it.
//! The shell contains panics at its boundaries (a command, a reader thread),
//! so a poisoned lock is the trace of a fault already reported — the data
//! under it is a plain value the next reader can use. Panicking again at
//! every later `lock()` would turn one contained fault into a cascade through
//! every command sharing the state. So the shell locks with
//! [`Locked::locked`] and never `lock().unwrap()`: a poisoned guard is taken
//! as it stands.

use std::sync::{Mutex, MutexGuard};

/// The one way the shell takes a `std::sync::Mutex`.
pub trait Locked<T> {
    /// The guard, poisoned or not: what a poisoned lock protects was left
    /// by a panic the shell already contained and reported.
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> Locked<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_is_taken_as_it_stands() {
        let m = std::sync::Arc::new(Mutex::new(vec![1, 2]));
        let poisoner = std::sync::Arc::clone(&m);
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = std::thread::spawn(move || {
            let mut g = poisoner.lock().unwrap();
            g.push(3);
            panic!("mid-write");
        })
        .join();
        std::panic::set_hook(quiet);
        assert!(panicked.is_err(), "the writer panicked mid-write");
        assert!(m.is_poisoned());
        assert_eq!(*m.locked(), vec![1, 2, 3], "the value survives the panic");
        m.locked().push(4);
        assert_eq!(m.locked().len(), 4);
    }
}
