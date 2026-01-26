# AST1060 Kernel Target

This target supports the ASPEED AST1060 SoC.

## Building the Image

To build the kernel image (ELF and Binary) for the `threads` example:

```sh
bazelisk build //pw_kernel/target/ast1060/threads/kernel:threads --config=k_qemu_ast1060
```

The output files will be located at:
- `bazel-bin/pw_kernel/target/ast1060/threads/kernel/threads.elf`
- `bazel-bin/pw_kernel/target/ast1060/threads/kernel/threads.bin`

## Running in QEMU

To run the image in QEMU and verify the UART console output:

```sh
bazelisk run //pw_kernel/target/ast1060/threads/kernel:threads --config=k_qemu_ast1060
```

You should see output demonstrating the UART console is working, such as:

```
Hello World from UART!
[INF] Welcome to the first thread, continuing bootstrap
[INF] Cortex-M initialization
...
```

## Implementation Details

- **UART Driver**: Located in `uart.rs`. Implements a basic 16550-compatible driver using `ast1060-pac`.
- **Console Backend**: Located in `console_backend.rs`. Initialized in `entry.rs` and provides the backend for `pw_log`.
- **Platform Config**: Defined in `BUILD.bazel`, mapping `console_backend` to the UART implementation.
