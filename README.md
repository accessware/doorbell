# doorbell

Keeps a desktop app to one running copy, and hands what a second copy was started with, such as files to open, to the first. Open a document while the app is already running, and it opens in the window that's already there.

```rust
let args: Vec<String> = std::env::args().skip(1).collect();
match doorbell::ring("myapp", &args) {
	// Another copy has the arguments, so this one is done.
	Ok(doorbell::Ring::Delivered) => return,
	Ok(doorbell::Ring::First(door)) => {
		door.listen(|message| {
			// On a background thread. Post it to the UI thread, then open each file, or bring the window to the front if the message is empty.
		});
		// Run the app.
	}
	Err(e) => eprintln!("Couldn't check for another copy: {e}"),
}
```

- A message is a list of strings, so a second copy can pass several files, or a command of your own. Turn relative paths into full ones before you send them, since the first copy may be running in another folder.
- `listen` calls you on a background thread. In a wxDragon app, pass the message on with [wx-utils](https://github.com/accessware/wx-utils)' `post_to_ui`.
- On Windows, the second copy lets the first bring its window to the front. Windows otherwise refuses that to a process the user isn't using.
- A copy that crashed or was killed never blocks the next one.

## How it works

| Platform | The first copy holds | Copies talk over |
| --- | --- | --- |
| Windows | A named pipe created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which only one process can do | That pipe. It refuses connections from other computers, and only the same user and administrators can write to it. |
| Linux and macOS | A lock on `<name>.lock` | A Unix socket, `<name>.sock`, beside the lock |

On Linux and macOS, both files are in `$XDG_RUNTIME_DIR` when it's set, and otherwise in a `doorbell-<user>` folder in the temp folder, which only the user can open. On macOS, the system already keeps an app bundle to one copy and sends it the files the user opens, so doorbell matters for copies started some other way, such as from a terminal.

To try it, run `cargo run --example demo` in one terminal, then `cargo run --example demo -- one "two three"` in another.

## License

MIT
