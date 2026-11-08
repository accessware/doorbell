//! Two copies of an app, played by two calls in one process: each call is a separate connection to the pipe or lock, as a second process's would be.

use std::{process, sync::mpsc, time::Duration};

use doorbell::Ring;

fn name(test: &str) -> String {
	format!("doorbell-test-{test}-{}", process::id())
}

#[test]
fn a_second_copy_hands_its_message_to_the_first() {
	let name = name("message");
	let Ring::First(door) = doorbell::ring(&name, &["first"]).unwrap() else { panic!("the first copy wasn't first") };
	let (sender, receiver) = mpsc::channel();
	door.listen(move |message| sender.send(message).unwrap());
	let second = doorbell::ring(&name, &["C:\\My Books\\one.epub", "https://example.org/two.pdf"]).unwrap();
	assert!(matches!(second, Ring::Delivered));
	assert_eq!(
		receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
		vec!["C:\\My Books\\one.epub".to_owned(), "https://example.org/two.pdf".to_owned()]
	);
	assert!(matches!(doorbell::ring(&name, &[] as &[&str]).unwrap(), Ring::Delivered));
	assert_eq!(receiver.recv_timeout(Duration::from_secs(10)).unwrap(), Vec::<String>::new());
}

#[test]
fn the_next_copy_is_first_once_the_first_is_gone() {
	let name = name("gone");
	let Ring::First(door) = doorbell::ring(&name, &[] as &[&str]).unwrap() else {
		panic!("the first copy wasn't first")
	};
	drop(door);
	assert!(matches!(doorbell::ring(&name, &[] as &[&str]).unwrap(), Ring::First(_)));
}

#[test]
fn a_name_that_would_leave_its_folder_is_refused() {
	assert!(doorbell::ring("../escape", &[] as &[&str]).is_err());
	assert!(doorbell::ring("", &[] as &[&str]).is_err());
}
