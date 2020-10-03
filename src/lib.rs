use std::alloc::{alloc, dealloc, Layout};
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use core::mem;

/// The max alignment types in an Arena can have. This will get replaced with constant generics
/// in the future once that is stable.
pub const MAX_ALIGN: usize = 16;

pub struct Arena {
	// INVARIANTS:
	// * buffer is an allocated block of memory with length bytes.
	buffer: *mut u8,
	length: usize,
}

impl Arena {
	/// Allocates a new arena with the specified length.
	///
	/// # Panics
	/// * If the given length is 0.
	/// * If the given length is not a multiple of [MAX_ALIGN].
	/// * If the allocation fails.
	pub fn new(length: usize) -> Self {
		assert!(length > 0, "length cannot be zero");
		assert_eq!(length & (MAX_ALIGN - 1), 0, "length has to be aligned to {}", MAX_ALIGN);

		// SAFETY: We know length is larger than zero.
		let buffer = unsafe { alloc(Layout::from_size_align(length, MAX_ALIGN).unwrap()) };
		assert!(!buffer.is_null(), "Allocation failed");

		Self {
			buffer,
			length,
		}
	}

	pub fn begin_alloc<'a>(&'a mut self) -> ArenaHead<'a> {
		ArenaHead {
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
			dealloc(self.buffer, Layout::from_size_align(self.length, MAX_ALIGN).unwrap());
		}
	}
}

pub struct ArenaHead<'a> {
	// INVARIANTS:
	// * The head must live for as long as 'a.
	// * The head must be allocated until ``last``
	head: *mut u8,
	last: *const u8,
	_phantom: PhantomData<&'a ()>,
}

impl<'a> ArenaHead<'a> {
	pub fn try_push<T>(&mut self, value: T) -> Option<ArenaBox<'a, T>> {
		match self.try_alloc(Layout::new::<T>()) {
			Some(ptr) => {
				unsafe {
					// SAFETY: We know that the pointer is valid because we just successfully
					// allocated it.
					(ptr as *mut T).write(value); 
					// SAFETY: We know that the raw pointer is not going to be accessed by anything
					// else, because we don't access it and the lifetimes ensure that the Arena
					// won't access it either.
					Some(ArenaBox::from_raw(ptr as *mut T))
				}
			}
			None => None,
		}
	}

	pub fn push<T>(&mut self, value: T) -> ArenaBox<'a, T> {
		self.try_push(value).unwrap()
	}

	#[inline]
	fn try_alloc(&mut self, layout: Layout) -> Option<*mut u8> {
		if layout.align() > MAX_ALIGN { return None; }

		// Because the alignemnt is smaller than MAX_ALIGN, it's not going to be unreasonably big.
		// Therefore, I think it's reasonable to assume this will never overflow.
		self.head = ((self.head as usize + layout.align() - 1) & !(layout.align() - 1)) as *mut u8;

		if self.last as usize - self.head as usize + 1 < layout.size() {
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

/// Similar to [std::boxed::Box] except it does not drop the memory location.
pub struct ArenaBox<'a, T> {
	// INVARIANT: buffer has to live for at least as long as 'a, it cannot be accessed by anything
	// else for 'a, it has to be non null and it has to point to a valid T.
	buffer: *mut T,
	_phantom: PhantomData<&'a mut T>,
}

impl<'a, T> ArenaBox<'a, T> {
	/// Creates a new box from a raw pointer.
	///
	/// # Safety
	/// The pointer has to be valid for 'a, and cannot be accessed by anything else during that time.
	pub unsafe fn from_raw(ptr: *mut T) -> Self {
		Self {
			buffer: ptr,
			_phantom: PhantomData,
		}
	}

	/// Returns a reference to the contained element.
	pub fn as_ref(&self) -> &T {
		// SAFETY: (from invariants)
		// self.buffer is only accessed by this struct, it is also nonnull and valid
		unsafe { &*self.buffer }
	}

	/// Returns a mutable reference to the contained element.
	pub fn as_mut(&mut self) -> &mut T {
		// SAFETY: (from invariants)
		// self.buffer is only accessed by this struct, it is also nonnull and valid
		unsafe { &mut *self.buffer }
	}

	/// Returns a raw pointer to the contained element.
	///
	/// # Guarantees
	/// The pointer that is returned will be non null and point to a valid instance of T, and will
	/// be valid for 'a.
	///
	/// # Safety
	/// * Do not read the pointer after another mutable borrow of this struct has occurred.
	pub fn as_ptr(&self) -> *const T {
		self.buffer
	}

	/// Returns a mutable raw pointer to the contained element.
	///
	/// # Guarantees
	/// The pointer that is returned will be non null and point to a valid instance of T, and will
	/// be valid for 'a.
	///
	/// # Safety
	/// * Do not write to the pointer after another mutable borrow of this struct has occurred.
	pub fn as_mut_ptr(&mut self) -> *mut T {
		self.buffer
	}

	/// Leaks the box. This does not return a 'static reference because [ArenaBox] does not own
	/// it's memory, hence this doesn't leak the memory which T resides in, but rather just doesn't
	/// call drop on T.
	pub fn leak(self) -> &'a mut T {
		let mut s = mem::ManuallyDrop::new(self);
		// This is safe for the same reason that ``as_mut`` is safe.
		unsafe { &mut *s.buffer }
	}
}

impl<T> std::fmt::Debug for ArenaBox<'_, T> where T: std::fmt::Debug {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		self.deref().fmt(f)
	}
}

impl<T> std::fmt::Display for ArenaBox<'_, T> where T: std::fmt::Display {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		self.as_ref().fmt(f)
	}
}

impl<T> Deref for ArenaBox<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		self.as_ref()
	}
}

impl<T> DerefMut for ArenaBox<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.as_mut()
	}
}

impl<T> Drop for ArenaBox<'_, T> {
	fn drop(&mut self) {
		// Drop the value inside. The buffer is managed by the arena, so we don't handle it here.
		let _ = unsafe { self.buffer.read() };
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

		let hello = allocator.push(5.2);
		allocator.push(5);

		// Without this drop, the next ``area.begin()`` will not work, because the drop call at the
		// end of the scope will try to drop hello, but the memory might have been overwritten.
		mem::drop(hello);

		let mut allocator = arena.begin_alloc();

		let my_string = allocator.push(format!("Hello, World!"));
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

		fn parse_stuff<'a>(arena: &mut ArenaHead<'a>) -> ArenaBox<'a, Ast<'a>> {
			let left  = arena.push(Ast::Number(125));
			let right = arena.push(Ast::Number(24));

			arena.push(Ast::BinaryOperator { left, right, operator: '+' })
		}

		let mut arena = Arena::new(1024);
		let mut arena = arena.begin_alloc();
		let ast = parse_stuff(&mut arena);

		println!("{:?}", ast);
	}
}
