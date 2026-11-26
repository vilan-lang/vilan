//! A struct key's ACCESS natively (B711 S1b, `proposal/struct-keys.md` §4 (a),
//! ruling R11): how a `Key<S, F>` held in a value reaches its field.
//!
//! On the JS backend a struct is an array in field order, so a key's steps
//! (its `KeyPath`) are enough to walk to the field. A native struct is a named
//! Rust struct with no offsets to walk, so a native key carries, beside the
//! path that gives it its identity, the two projections that ARE the place:
//! `for<'a> Fn(&'a S) -> &'a F` and its `&mut` twin. Views are Rust references
//! natively, so `&s[k]` and `&mut s[k]` are exactly these projections applied.
//!
//! - A key LITERAL (`Task.meta.owner`) is [`KeyAccess::Field`]: two
//!   non-capturing functions the emitter writes at the literal, so building
//!   one allocates nothing and copying one is two pointer copies.
//! - `k.then(next)` is [`KeyAccess::Composed`]: one `Rc` per composition
//!   holding copies of both halves, so a copy of a composed key is a
//!   reference-count bump. A key holds no cell and no back edge, so nothing it
//!   holds can form a cycle.
//!
//! A subscript through a literal never builds a `KeyAccess` at all: the
//! emitter writes the plain field chain (`s.meta.owner`), as the JS emitter
//! does.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// C14's leak gate, for keys: compositions made and dropped. A key holds no
/// cell, so nothing it holds can close a cycle, and at process end every
/// composition must be gone; [`key_census`] lets the native leak census say
/// so (`main_guard` prints it beside the cells' line once a program composed
/// any).
static COMPOSITIONS_MINTED: AtomicU64 = AtomicU64::new(0);
static COMPOSITIONS_RELEASED: AtomicU64 = AtomicU64::new(0);

/// `(minted, released)` key compositions so far.
pub fn key_census() -> (u64, u64) {
    (
        COMPOSITIONS_MINTED.load(Ordering::Relaxed),
        COMPOSITIONS_RELEASED.load(Ordering::Relaxed),
    )
}

/// The two projections of a key from `S` to its field of type `F`.
pub enum KeyAccess<S: 'static, F: 'static> {
    /// A literal's: two non-capturing functions, `|s| &s.a.b` and
    /// `|s| &mut s.a.b`.
    Field(fn(&S) -> &F, fn(&mut S) -> &mut F),
    /// A composition's (`then`): the halves behind one shared allocation.
    Composed(Rc<dyn Reach<S, F>>),
}

/// A projection from `S` to `F`, both ways. [`KeyAccess`] implements it, and
/// so does a composition of two of them, which is what lets a composition
/// erase the type in the middle (`Key<S, M>` then `Key<M, F>`).
pub trait Reach<S, F> {
    /// The field, read.
    fn reach<'a>(&self, subject: &'a S) -> &'a F;
    /// The field, written.
    fn reach_mut<'a>(&self, subject: &'a mut S) -> &'a mut F;
}

impl<S: 'static, F: 'static> KeyAccess<S, F> {
    /// A literal's access, from its two projections.
    pub fn field(read: fn(&S) -> &F, write: fn(&mut S) -> &mut F) -> Self {
        KeyAccess::Field(read, write)
    }

    /// This access followed by `next`'s: `Task.meta` then `Meta.owner`.
    pub fn then<G: 'static>(&self, next: &KeyAccess<F, G>) -> KeyAccess<S, G> {
        COMPOSITIONS_MINTED.fetch_add(1, Ordering::Relaxed);
        KeyAccess::Composed(Rc::new(Then {
            first: self.clone(),
            next: next.clone(),
        }))
    }
}

impl<S: 'static, F: 'static> Reach<S, F> for KeyAccess<S, F> {
    fn reach<'a>(&self, subject: &'a S) -> &'a F {
        match self {
            KeyAccess::Field(read, _) => read(subject),
            KeyAccess::Composed(composed) => composed.reach(subject),
        }
    }

    fn reach_mut<'a>(&self, subject: &'a mut S) -> &'a mut F {
        match self {
            KeyAccess::Field(_, write) => write(subject),
            KeyAccess::Composed(composed) => composed.reach_mut(subject),
        }
    }
}

/// Copying a key copies its access: two function pointers, or one count.
impl<S: 'static, F: 'static> Clone for KeyAccess<S, F> {
    fn clone(&self) -> Self {
        match self {
            KeyAccess::Field(read, write) => KeyAccess::Field(*read, *write),
            KeyAccess::Composed(composed) => KeyAccess::Composed(Rc::clone(composed)),
        }
    }
}

/// A key's identity is its PATH, which the struct beside the access compares;
/// the access is what the path already determines (K3: keys with equal paths
/// reach the same place). So two accesses are always equal here, and the
/// emitted struct's derived `PartialEq` compares the paths alone.
impl<S: 'static, F: 'static> PartialEq for KeyAccess<S, F> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// `first` then `next`, erasing the middle type.
struct Then<S: 'static, M: 'static, F: 'static> {
    first: KeyAccess<S, M>,
    next: KeyAccess<M, F>,
}

/// A composition going away is a RELEASE, counted for the leak gate.
impl<S: 'static, M: 'static, F: 'static> Drop for Then<S, M, F> {
    fn drop(&mut self) {
        COMPOSITIONS_RELEASED.fetch_add(1, Ordering::Relaxed);
    }
}

impl<S: 'static, M: 'static, F: 'static> Reach<S, F> for Then<S, M, F> {
    fn reach<'a>(&self, subject: &'a S) -> &'a F {
        self.next.reach(self.first.reach(subject))
    }

    fn reach_mut<'a>(&self, subject: &'a mut S) -> &'a mut F {
        self.next.reach_mut(self.first.reach_mut(subject))
    }
}

/// `s[k]` read through a key held in a value: the field, borrowed from `s`.
/// The emitter hands it a COPY of the key's access (or a temp a hoist settled
/// one in), so the subscript holds no borrow of the key while it borrows the
/// subject: `s[s.k]` is fine.
pub fn key_ref<'a, S: 'static, F: 'static>(access: &KeyAccess<S, F>, subject: &'a S) -> &'a F {
    access.reach(subject)
}

/// `s[k]` as a place written through a key held in a value.
pub fn key_mut<'a, S: 'static, F: 'static>(
    access: &KeyAccess<S, F>,
    subject: &'a mut S,
) -> &'a mut F {
    access.reach_mut(subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Meta {
        owner: String,
        stars: i32,
    }

    struct Task {
        title: String,
        meta: Meta,
    }

    fn task() -> Task {
        Task {
            title: "a".to_string(),
            meta: Meta {
                owner: "ada".to_string(),
                stars: 1,
            },
        }
    }

    #[test]
    fn keys_a_literal_access_reads_and_writes_its_field() {
        let title = KeyAccess::<Task, String>::field(|s| &s.title, |s| &mut s.title);
        let mut subject = task();
        *key_mut(&title, &mut subject) = "b".to_string();
        assert_eq!(key_ref(&title, &subject), "b");
    }

    #[test]
    fn keys_a_composed_access_reaches_the_leaf_and_shares_its_halves() {
        let meta = KeyAccess::<Task, Meta>::field(|s| &s.meta, |s| &mut s.meta);
        let stars = KeyAccess::<Meta, i32>::field(|s| &s.stars, |s| &mut s.stars);
        let owner = KeyAccess::<Meta, String>::field(|s| &s.owner, |s| &mut s.owner);
        let composed = meta.then(&stars);
        let copy = composed.clone();
        let mut subject = task();
        *key_mut(&copy, &mut subject) += 4;
        assert_eq!(*key_ref(&composed, &subject), 5);
        let deeper = meta.then(&owner);
        key_mut(&deeper, &mut subject).push('!');
        assert_eq!(key_ref(&deeper, &subject), "ada!");
        let KeyAccess::Composed(shared) = &composed else {
            panic!("a composition is one shared allocation");
        };
        assert_eq!(Rc::strong_count(shared), 2, "a copy shares the composition");
        drop(copy);
        assert_eq!(Rc::strong_count(shared), 1);
        assert_eq!(subject.title, "a");
    }
}
