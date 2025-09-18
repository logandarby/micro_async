Alternate scheme (Less error prone)

- When something like an input channel is polled, its channel in a static array is set to the waker from the poll_fn context
- So, when a task is awoken inside an interrupt, the wake() function is called.
- Now, the VTABLE has been registered such that the waker is awoken, the wake_task function is called, which enqueues the function with the executor.
    - Note the executor reference is stored in the reference to the task.
    - See that the task reference is the data inside the waker. The task is essentially stored statically through the task macro
- Now after all that is done, the WFI/WFE instruction from the executors main poll loop is finally over, so it can go through all the queued events.

Other considerations
- The runtime queue should be a linked list that reference the statically allocated taskrefs


Embassy (and Rust async in general) uses careful patterns to ensure safety when passing TaskRef pointers through waker vtables, even though this involves *const () (void pointers) and unsafe code.

Here’s how UB is avoided:

    1. The TaskRef is always a pointer to a statically allocated TaskHeader (inside a TaskStorage), which is 'static and never deallocated.
    2. When a waker is created for a task, the pointer to the TaskHeader (or a type-erased TaskRef) is stored in the waker’s data field (as a *const ()).
    3. The waker vtable functions (like wake, clone, drop) cast this *const () back to a TaskRef or &'static TaskHeader using unsafe, but this is safe because:
        - The pointer always points to a valid, live, statically allocated object.
        - The pointer is never used after the task is dropped, because tasks are never deallocated (they are reused or remain in memory for the program’s lifetime).
        - The pointer is never aliased in a way that violates Rust’s aliasing rules, because the executor and waker system are designed to only access the task from one place at a time.
    4. No stack or heap memory is involved, so there’s no risk of dangling pointers.

This pattern is a well-established approach in async runtimes and is considered safe as long as:

    - The pointer always points to a valid, 'static object.
    - The object is never deallocated or moved.
    - The pointer is only used for the intended task.
    - Embassy’s macros and executor design guarantee these i
