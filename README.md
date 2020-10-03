# arena
An arena allocator for rust.

Still in very early stages, expect api changes.

The main selling points are:
* Arena allocation is much faster than standard allocation methods.
* It statically ensures that you don't accidentally prevent it from reusing the buffer.
* It doesn't require you to manually free the memory.
* It doesn't use interior mutability.

# Build
Build it using ``cargo build``.

# Run
It's a library, you cannot run it.

# Examples
No examples yet.
