use std::{path::PathBuf, collections::HashMap, cell::{RefCell, Cell}, io::ErrorKind, fs::File};

use crate::common::OSErr;

pub struct FSSpec {
	pub parent_id: u32,
	pub name: String,
	pub is_folder: bool,
	pub exists: bool
}

pub struct FileSystem {
	node_ids: RefCell<HashMap<PathBuf, u32>>,
	next_node_id: Cell<u32>
}

pub enum FSResult<T> {
	Ok(T),
	Err(OSErr)
}

impl FileSystem {
	pub fn new() -> Self {
		FileSystem {
			node_ids: RefCell::new(HashMap::new()),
			next_node_id: Cell::new(10)
		}
	}
	
	pub fn get_node_id(&self, path: &PathBuf) -> u32 {
		if let Some(id) = self.node_ids.borrow().get(path) {
			return *id;
		}

		let id = if path.file_name().is_some() {
			let id = self.next_node_id.get();
			self.next_node_id.set(id + 1);
			id
		} else {
			// fix the root directory to 2
			2
		};

		debug!(target: "fs", "{id} => {path:?}");
		self.node_ids.borrow_mut().insert(path.clone(), id);
		id
	}

	pub fn get_path_for_node_id(&self, id: u32) -> Option<PathBuf> {
		for (entry_path, entry_id) in self.node_ids.borrow().iter() {
			if *entry_id == id {
				return Some(entry_path.clone());
			}
		}
		None
	}

	pub fn make_resolved_fs_spec(&self, directory: u32, mac_path: &str) -> Option<FSSpec> {
		assert_eq!(directory, 0);

		// Create our own path
		let mut path = if mac_path == ":" {
			PathBuf::from(".")
			//std::env::current_dir().expect("Should be able to get current directory")
		} else {
			PathBuf::from(mac_path)
		};

		path = match path.canonicalize() {
			Ok(p) => p,
			Err(e) if e.kind() == ErrorKind::NotFound => return None,
			Err(e) => Err(e).expect("Should be able to canonicalise path")
		};

		let is_folder = path.is_dir();
		let name = path.file_name().expect("Should have a name").to_string_lossy().to_string();
		path.pop();
		let parent_id = self.get_node_id(&path);

		Some(FSSpec { parent_id, name, is_folder, exists: true })
	}
	
	pub fn make_fs_spec(&self, directory: u32, mac_path: &str) -> FSResult<FSSpec> {
		// What do?
		let mut path = if directory == 0 {
			PathBuf::from(".")
		} else {
			self.get_path_for_node_id(directory).expect("Directory should be known")
		};
		path.push(mac_path.replace(':', "/"));
		
		let exists = path.exists();
		let is_folder = path.is_dir();

		let name = path.file_name().expect("Should have a name").to_string_lossy().to_string();
		path.pop();

		path = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => return FSResult::Err(OSErr::DirNotFound)
		};
		
		let parent_id = self.get_node_id(&path);

		FSResult::Ok(FSSpec { parent_id, name, is_folder, exists })
	}

	pub fn h_create(&self, directory: u32, mac_path: &str) -> FSResult<()> {
		let mut path = self.get_path_for_node_id(directory).expect("wanna see a parent dir for HOpen");
		path.push(mac_path);

		match File::create(path) {
			Ok(_) => FSResult::Ok(()),
			Err(_) => FSResult::Err(OSErr::IOError)
		}
	}

	pub fn h_open(&self, directory: u32, mac_path: &str, permission: i8) -> FSResult<File> {
		let mut path = self.get_path_for_node_id(directory).expect("wanna see a parent dir for HOpen");
		path.push(mac_path);

		let r = File::options()
			.read(permission == 1 || permission == 3 || permission == 4)
			.write(permission >= 2 && permission <= 4)
			.append(permission >= 2 && permission <= 4)
			.create(permission >= 2 && permission <= 4)
			.open(path);

		match r {
			Ok(file) => FSResult::Ok(file),
			Err(e) => {
				error!(target: "fs", "Error opening file: {e:?}");
				match e.kind() {
					ErrorKind::NotFound => FSResult::Err(OSErr::FileNotFound), // fnfErr
					_ => FSResult::Err(OSErr::IOError) // ioErr
				}
			}
		}
	}

	pub fn get_subnode_info(&self, parent_id: u32, name: &str) -> FSResult<Info> {
		let mut path = self.get_path_for_node_id(parent_id).expect("directory id must exist");
		path.push(name);

		if path.exists() {
			if path.is_dir() {
				let id = self.get_node_id(&path);
				FSResult::Ok(Info::Directory { id, parent_id, name: None })
			} else {
				FSResult::Ok(Info::File { parent_id, name: None })
			}
		} else {
			FSResult::Err(OSErr::FileNotFound)
		}
	}

	pub fn get_info_by_id(&self, id: u32) -> FSResult<Info> {
		let mut path = self.get_path_for_node_id(id).expect("id not known");

		let name = path.file_name().expect("Should have a name").to_string_lossy().to_string();
		let is_dir = path.is_dir();
		path.pop();
		let parent_id = self.get_node_id(&path);

		if is_dir {
			FSResult::Ok(Info::Directory { id, parent_id, name: Some(name) })
		} else {
			FSResult::Ok(Info::File { parent_id, name: Some(name) })
		}
	}
}

pub enum Info {
	Directory { id: u32, parent_id: u32, name: Option<String> },
	File { parent_id: u32, name: Option<String> }
}
