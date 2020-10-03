//! This crate is another arena allocator.
//!
//! The main selling points are:
//! * Arena allocation is much faster than standard allocation methods.
//! * It statically ensures that you don't accidentally prevent it from reusing the buffer.
//! * It doesn't require you to manually free the memory.
//! * It doesn't use interior mutability.
//!
//! The [Arena] struct manages the memory that is then used for allocating. It doesn't allocate
//! anything on it's own, however. For that, you use the [ArenaAlloc] struct.
//!
//! The [ArenaAlloc] struct uses the memory from an [Arena] to allocate
//! [ArenaBox]es(among other, more primitive things). There can only be one [ArenaAlloc] per [Arena]
//! at a time, which is ensured statically with the borrowing rules.
//!
//! An [ArenaBox] works exactly like a [Box] except it has a lifetime, and it drops the thing it
//! contains.
//!
#[warn(missing_docs)]

use std::alloc::{alloc, dealloc, Layout};
use std::marker::PhantomData;

mod r#box;
pub use r#box::ArenaBox;

/// A buffer that contains heap allocated memory that can be used by the [ArenaAlloc].
pub struct Arena {
	// INVARIANTS:
	// * buffer is an allocated block of memory with length bytes.
	buffer: *mut u8,
	length: usize,
}

impl Arena {
	/// Allocates a new arena with the specified length.
	///
	/// The length might be a little misleading, because this buffer doesn't have a specified
	/// alignment. This means that the buffer may not be able to contain the exact same number of 
	/// elements eveery time, because there may have to be a different amount of padding needed.
	///
	/// # Panics
	/// * If the given length is 0.
	/// * If the allocation fails.
	pub fn new(length: usize) -> Self {
		assert!(length > 0, "length cannot be zero");

		// SAFETY: We know length is larger than zero.
		let buffer = unsafe { alloc(Layout::from_size_align(length, 1).unwrap()) };
		assert!(!buffer.is_null(), "Allocation failed");

		Self {
			buffer,
			length,
		}
	}

	/// Allows allocating elements from the start of the buffer.
	///
	/// This can be called multiple times
	/// to reuse the same buffer for several batches of allocations, however, it is statically
	/// guaranteed that no allocations from one batch can live to the next batch.
	pub fn begin_alloc<'a>(&'a mut self) -> ArenaAlloc<'a> {
		ArenaAlloc {
			head: self.buffer,
			// SAFETY: Because self.buffer is an allocation of self.length elements,
			// self.length - 1 will never overflow. self.length is also larger than zero,
			// which means
			last: unsafe { self.buffer.add(self.length - 1) },
			_phantom: PhantomData,
		}
	}
}

impl Drop for Arena {
	fn drop(&mut self) {
		// SAFETY: We never change the length from the new method, hence we know it's not zero
		// and that the layout is the exact same as the one we allocated with.
		unsafe {
			dealloc(self.buffer, Layout::from_size_align(self.length, 1).unwrap());
		}
	}
}

/// Allocates items into an [Arena].
pub struct ArenaAlloc<'a> {
	// INVARIANTS:
	// * The head must live for as long as 'a.
	// * The head must be allocated until ``last``
	head: *mut u8,
	last: *const u8,
	_phantom: PhantomData<&'a ()>,
}

impl<'a> ArenaAlloc<'a> {
	/// Tries to allocate a space for T and insert the value into it. If there isn't enough space
	/// for T, it will return None.
	pub fn try_insert<T>(&mut self, value: T) -> Option<ArenaBox<'a, T>> {
		match self.try_alloc::<T>() {
			Some(ptr) => {
				unsafe {
					// SAFETY: We know that the pointer is valid because we just successfully
					// allocated it.
					ptr.write(value); 
					// SAFETY: We know that the raw pointer is not going to be accessed by anything
					// else, because we don't access it and the lifetimes ensure that the Arena
					// won't access it either.
					Some(ArenaBox::from_raw(ptr))
				}
			}
			None => None,
		}
	}

	/// Tries to allocate a space for T and insert the value into it.
	///
	/// # Panics
	/// * If there isn't enough space in the [Arena].
	pub fn insert<T>(&mut self, value: T) -> ArenaBox<'a, T> {
		self.try_insert(value).expect("Not enough space for to insert a value")
	}

	/// Tries to allocate a raw pointer to a T. This raw pointer is guaranteed to be valid and to
	/// not be accessed by anything else for the lifetime 'a. Returns None if there is not enough
	/// space.
	pub fn try_alloc<T>(&mut self) -> Option<*mut T> {
		self.try_alloc_layout(Layout::new::<T>()).map(|v| v as *mut T)
	}

	/// Tries to allocate a raw pointer to a T. This raw pointer is guaranteed to be valid and to
	/// not be accessed by anything else for the lifetime 'a.
	///
	/// # Panics
	/// * If there is not enough space for a T in the Arena.
	pub fn alloc<T>(&mut self) -> *mut T {
		self.try_alloc::<T>().expect("Not enough space")
	}

	#[inline]
	fn try_alloc_layout(&mut self, layout: Layout) -> Option<*mut u8> {
		// TODO: We may want to be less pedantic here for performance reasons.
		// (layout.align() - 1) is fine because align is guaranteed to not be zero.
		self.head = (
			(self.head as usize).checked_add(layout.align() - 1)?
			& !(layout.align() - 1)
		) as *mut u8;

		// self.last is always larger than self.head, so this will never overflows.
		if self.last as usize - self.head as usize <= layout.size() {
			return None;
		}

		let value = self.head;
		// SAFETY: We know that head + size does not go past the allocation point, and the allocation
		// has to not overflow.
		unsafe {
			self.head = self.head.add(layout.size());
		}
		Some(value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn create_arena() {
		let _arena = Arena::new(512);
	}

	#[test]
	fn allocate_numbers() {
		let mut arena = Arena::new(512);
		let mut allocator = arena.begin_alloc();

		let hello = allocator.insert(5.2);
		allocator.insert(5);

		// Without this drop, the next ``area.begin()`` will not work, because the drop call at the
		// end of the scope will try to drop hello, but the memory might have been overwritten.
		std::mem::drop(hello);

		let mut allocator = arena.begin_alloc();

		let my_string = allocator.insert(format!("Hello, World!"));
		println!("{}", my_string);
	}

	#[test]
	fn enum_testing() {
		#[derive(Debug)]
		enum Ast<'a> {
			Number(i64),
			BinaryOperator {
				left:  ArenaBox<'a, Ast<'a>>,
				right: ArenaBox<'a, Ast<'a>>,
				operator: char,
			}
		}

		fn parse_stuff<'a>(arena: &mut ArenaAlloc<'a>) -> ArenaBox<'a, Ast<'a>> {
			let left  = arena.insert(Ast::Number(125));
			let right = arena.insert(Ast::Number(24));

			arena.insert(Ast::BinaryOperator { left, right, operator: '+' })
		}

		let mut arena = Arena::new(1024);
		let mut arena = arena.begin_alloc();
		let ast = parse_stuff(&mut arena);

		println!("{:?}", ast);
	}

	#[test]
	fn mass_allocate() {
		let mut arena = Arena::new(9000);
		let mut insert = arena.begin_alloc();

		let _: Vec<_> = (0..1000u64).map(|v| insert.insert(v)).collect();
	}

	#[should_panic]
	#[test]
	fn over_allocate() {
		let mut arena = Arena::new(16);
		let mut insert = arena.begin_alloc();
		insert.insert(5u64);
		insert.insert(5u64);
		insert.insert(5u64);
	}
}
