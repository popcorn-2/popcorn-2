mod linked_list;
mod doubly_linked_list;

pub use linked_list::SinglyLinkedList;
pub use doubly_linked_list::DoublyLinkedList;

use crate::task::OwnedTask;

pub enum PopResult {
	None,
	Some(OwnedTask),
	Outdated(OwnedTask),
}
