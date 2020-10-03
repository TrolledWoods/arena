use std::alloc;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use core::mem;

pub struct Arena {
	buffer: *mut u8,
	length: usize,
}

impl Arena {
	pub fn new(length: usize) -> Self {
		assert!(length > 0, "Length cannot be zero");

		let buffer = unsafe { alloc::alloc(alloc::Layout::from_size_align(length, 16).unwrap()) };
		assert!(!buffer.is_null(), "Allocation failed");

		Self {
			// SAFETY: We know that the length is larger than 0
			buffer,
			length,
		}
	}

	pub fn begin_alloc<'a>(&'a mut self) -> ArenaHead<'a> {
		ArenaHead {
			head: self.buffer,
			// TODO: Figure out safety of this.
			max: unsafe { self.buffer.add(self.length) },
			_phantom: PhantomData,
		}
	}
}

impl Drop for Arena {
	fn drop(&mut self) {
		// SAFETY: We never change the length from the new method, hence we know it's not zero
		// and that the layout is the exact same as the one we allocated with.
		unsafe {
			alloc::dealloc(self.buffer, alloc::Layout::from_size_align(self.length, 16).unwrap());
		}
	}
}

pub struct ArenaHead<'a> {
	// INVARIANT: The head must live for as long as 'a.
	head: *mut u8,
	max: *const u8,
	_phantom: PhantomData<&'a ()>,
}

impl<'a> ArenaHead<'a> {
	pub fn try_push<T>(&mut self, value: T) -> Option<Box<'a, T>> {
		let align = mem::align_of::<T>();
		let size  = mem::size_of::<T>();

		// Align the head
		self.head = ((self.head as usize + align - 1) & !(align - 1)) as *mut u8;

		assert!(self.head as usize + size < self.max as usize);

		// We know the raw pointer is valid for at least 'a, that the head is aligned to the align
		// of T, and that it is allocated for enough space for a 'T' to fit.
		unsafe {
			// We use write to make sure it doesn't drop the value in 'self.head'
			(self.head as *mut T).write(value);
		}

		let value = Box {
			buffer: self.head as *mut T,
			_phantom: PhantomData,
		};

		// TODO: Make this completely safe
		unsafe {
			self.head = self.head.add(size);
		}

		Some(value)
	}

	pub fn push<T>(&mut self, value: T) -> Box<'a, T> {
		self.try_push(value).unwrap()
	}
}

/// This box owns a value inside someone elses block of memory. Essentially, it's a ``&'a mut T``
/// except it drops T.
pub struct Box<'a, T> {
	buffer: *mut T,
	_phantom: PhantomData<&'a mut T>,
}

impl<T> std::fmt::Debug for Box<'_, T> where T: std::fmt::Debug {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		self.deref().fmt(f)
	}
}

impl<T> std::fmt::Display for Box<'_, T> where T: std::fmt::Display {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		self.deref().fmt(f)
	}
}

impl<T> Deref for Box<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.buffer }
	}
}

impl<T> DerefMut for Box<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { &mut *self.buffer }
	}
}

impl<T> Drop for Box<'_, T> {
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
		std::mem::drop(hello);

		let mut allocator = arena.begin_alloc();

		let my_string = allocator.push(format!("Hello, World!"));
		println!("{}", my_string);
	}
}
