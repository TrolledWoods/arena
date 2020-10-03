use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::mem;
use std::iter::FusedIterator;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::{Read, Write, BufRead, IoSliceMut, IoSlice, SeekFrom, Seek};
use std::fmt;
use std::future::Future;
use std::task::{Poll, Context};
use std::pin::Pin;
use std::borrow::{Borrow, BorrowMut};

/// Similar to [Box] except it does not drop the memory location.
pub struct ArenaBox<'a, T: ?Sized> {
	// INVARIANT: buffer has to live for at least as long as 'a, it cannot be accessed by anything
	// else for 'a, and it has to point to a valid T.
	buffer: *mut T,
	_phantom: PhantomData<&'a mut T>,
}

impl<'a, T> ArenaBox<'a, T> where T: ?Sized {
	/// Creates a new box from a raw pointer. This box will not free the given pointer when dropped!
	///
	/// # Safety
	/// * The pointer has to be valid for 'a
	/// * It cannot be accessed by anything else during that time
	/// * It has to point to a valid T.
	pub unsafe fn from_raw(ptr: *mut T) -> Self {
		Self {
			buffer: ptr,
			_phantom: PhantomData,
		}
	}

	/// Converts this into a raw pointer.
	/// 
	/// # Guarantees
	/// * The pointer is not null 
	/// * The pointer points to a valid T
	/// * The pointer is valid for 'a
	///
	/// # Safety
	/// * Do not free the pointer, that may cause a double free.
	pub fn into_raw(self) -> *mut T {
		mem::ManuallyDrop::new(self).buffer
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
	/// * The pointer is not null 
	/// * The pointer points to a valid T
	/// * The pointer is valid for 'a
	pub fn as_ptr(&self) -> *const T {
		self.buffer
	}

	/// Returns a mutable raw pointer to the contained element.
	///
	/// # Guarantees
	/// * The pointer is not null 
	/// * The pointer points to a valid T
	/// * The pointer is valid for 'a
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

impl<T> fmt::Debug for ArenaBox<'_, T> where T: fmt::Debug + ?Sized {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.as_ref().fmt(f)
	}
}

impl<T> fmt::Display for ArenaBox<'_, T> where T: fmt::Display + ?Sized {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.as_ref().fmt(f)
	}
}

impl<T: ?Sized> Borrow<T> for ArenaBox<'_, T> {
	fn borrow(&self) -> &T {
		self.as_ref()
	}
}

impl<T: ?Sized> BorrowMut<T> for ArenaBox<'_, T> {
	fn borrow_mut(&mut self) -> &mut T {
		self.as_mut()
	}
}

impl<T: ?Sized> Deref for ArenaBox<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		self.as_ref()
	}
}

impl<T: ?Sized> DerefMut for ArenaBox<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.as_mut()
	}
}

impl<T: ?Sized> Drop for ArenaBox<'_, T> {
	fn drop(&mut self) {
		unsafe {
			std::ptr::drop_in_place(self.buffer);
		}
	}
}

impl<T> std::convert::AsMut<T> for ArenaBox<'_, T> {
	fn as_mut(&mut self) -> &mut T {
		&mut *self
	}
}

impl<T> std::convert::AsRef<T> for ArenaBox<'_, T> {
	fn as_ref(&self) -> &T {
		&*self
	}
}

impl<T> Iterator for ArenaBox<'_, T> where T: Iterator + ?Sized {
	type Item = T::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.as_mut().next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.as_ref().size_hint()
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.as_mut().nth(n)
    }
	// TODO: Can we implement last here somehow?
}

impl<T> DoubleEndedIterator for ArenaBox<'_, T> where T: DoubleEndedIterator + ?Sized {
	fn next_back(&mut self) -> Option<Self::Item> {
		self.as_mut().next()
	}
}

impl<T> ExactSizeIterator for ArenaBox<'_, T> where T: ExactSizeIterator + ?Sized {
	fn len(&self) -> usize {
		self.as_ref().len()
	}
}

impl<T> FusedIterator for ArenaBox<'_, T> where T: FusedIterator + ?Sized { }

impl<T> Hash for ArenaBox<'_, T> where T: Hash + ?Sized {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl<T> Hasher for ArenaBox<'_, T> where T: Hasher + ?Sized {
    fn finish(&self) -> u64 {
        self.as_ref().finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.as_mut().write(bytes)
    }
    fn write_u8(&mut self, i: u8) {
        self.as_mut().write_u8(i)
    }
    fn write_u16(&mut self, i: u16) {
        self.as_mut().write_u16(i)
    }
    fn write_u32(&mut self, i: u32) {
        self.as_mut().write_u32(i)
    }
    fn write_u64(&mut self, i: u64) {
        self.as_mut().write_u64(i)
    }
    fn write_u128(&mut self, i: u128) {
        self.as_mut().write_u128(i)
    }
    fn write_usize(&mut self, i: usize) {
        self.as_mut().write_usize(i)
    }
    fn write_i8(&mut self, i: i8) {
        self.as_mut().write_i8(i)
    }
    fn write_i16(&mut self, i: i16) {
        self.as_mut().write_i16(i)
    }
    fn write_i32(&mut self, i: i32) {
        self.as_mut().write_i32(i)
    }
    fn write_i64(&mut self, i: i64) {
        self.as_mut().write_i64(i)
    }
    fn write_i128(&mut self, i: i128) {
        self.as_mut().write_i128(i)
    }
    fn write_isize(&mut self, i: isize) {
        self.as_mut().write_isize(i)
    }
}

impl<R: Read + ?Sized> Read for ArenaBox<'_, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.as_mut().read(buf)
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.as_mut().read_vectored(bufs)
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.as_mut().read_to_end(buf)
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.as_mut().read_to_string(buf)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.as_mut().read_exact(buf)
    }
}

impl<W: Write + ?Sized> Write for ArenaBox<'_, W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.as_mut().write(buf)
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.as_mut().write_vectored(bufs)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.as_mut().flush()
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.as_mut().write_all(buf)
    }

    #[inline]
    fn write_fmt(&mut self, fmt: fmt::Arguments<'_>) -> io::Result<()> {
        self.as_mut().write_fmt(fmt)
    }
}

impl<S: Seek + ?Sized> Seek for ArenaBox<'_, S> {
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.as_mut().seek(pos)
    }
}

impl<T> BufRead for ArenaBox<'_, T> where T: BufRead + ?Sized {
    #[inline]
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.as_mut().fill_buf()
    }

    #[inline]
    fn consume(&mut self, amt: usize) {
        self.as_mut().consume(amt)
    }

    #[inline]
    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.as_mut().read_until(byte, buf)
    }

    #[inline]
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        self.as_mut().read_line(buf)
    }
}

impl<T: ?Sized> Unpin for ArenaBox<'_, T> {}

impl<T: ?Sized + PartialEq> PartialEq for ArenaBox<'_, T> {
    #[inline]
    fn eq(&self, other: &ArenaBox<'_, T>) -> bool {
        PartialEq::eq(self.as_ref(), other.as_ref())
    }
    #[inline]
    fn ne(&self, other: &ArenaBox<T>) -> bool {
        PartialEq::ne(self.as_ref(), other.as_ref())
    }
}

impl<T: ?Sized + PartialOrd> PartialOrd for ArenaBox<'_, T> {
    #[inline]
    fn partial_cmp(&self, other: &ArenaBox<'_, T>) -> Option<std::cmp::Ordering> {
        PartialOrd::partial_cmp(self.as_ref(), other.as_ref())
    }
    #[inline]
    fn lt(&self, other: &ArenaBox<'_, T>) -> bool {
        PartialOrd::lt(self.as_ref(), other.as_ref())
    }
    #[inline]
    fn le(&self, other: &ArenaBox<'_, T>) -> bool {
        PartialOrd::le(self.as_ref(), other.as_ref())
    }
    #[inline]
    fn ge(&self, other: &ArenaBox<'_, T>) -> bool {
        PartialOrd::ge(self.as_ref(), other.as_ref())
    }
    #[inline]
    fn gt(&self, other: &ArenaBox<'_, T>) -> bool {
        PartialOrd::gt(self.as_ref(), other.as_ref())
    }
}

impl<T: ?Sized + Eq> Eq for ArenaBox<'_, T> {}

impl<F: ?Sized + Future + Unpin> Future for ArenaBox<'_, F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        F::poll(Pin::new(self.get_mut().as_mut()), cx)
    }
}
