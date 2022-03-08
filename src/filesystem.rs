use std::{cell::RefCell, collections::HashMap, path::{PathBuf, Path, Prefix}, io::{Read, Cursor, Write}, fs::File, ffi::OsString, rc::Rc};

use anyhow::{anyhow, Result};
use bimap::BiHashMap;
use binread::{BinRead, BinReaderExt};
use xattr::FileExt;

use crate::{common::{FourCC, four_cc}, macbinary, mac_roman};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fork {
	Data,
	Resource
}

type VolumeRef = i16;
type DirID = i32;
type VolumeAndDir = (VolumeRef, DirID);

const ROOT_PARENT_DIR_ID: DirID = 1;
const ROOT_DIR_ID: DirID = 2;

enum FileMode {
	// Data fork only, type/creator ID determined from extension
	Automatic,
	// MacBinary III
	MacBinary,
	// Use the native info in file system attributes
	Native
}

#[derive(BinRead)]
pub struct FileInfo {
	pub file_type: FourCC,
	pub file_creator: FourCC,
	pub finder_flags: u16,
	pub location: (i16, i16),
	pub reserved_field: u16,
	pub extended_data: [u8; 16]
}

impl FileInfo {
	fn pack(&self) -> Vec<u8> {
		let mut data = Vec::with_capacity(32);
		data.extend(self.file_type.0.to_be_bytes());
		data.extend(self.file_creator.0.to_be_bytes());
		data.extend(self.finder_flags.to_be_bytes());
		data.extend(self.location.0.to_be_bytes());
		data.extend(self.location.1.to_be_bytes());
		data.extend(self.reserved_field.to_be_bytes());
		data.extend(self.extended_data);
		data
	}
}

pub struct MacFile {
	pub path: PathBuf,
	mode: FileMode,
	dirty: bool,
	pub file_info: FileInfo,
	pub data_fork: Vec<u8>,
	pub resource_fork: Vec<u8>
}

impl MacFile {
	pub fn len(&self, fork: Fork) -> usize {
		match fork {
			Fork::Data => self.data_fork.len(),
			Fork::Resource => self.resource_fork.len()
		}
	}

	pub fn get_fork(&self, fork: Fork) -> &Vec<u8> {
		match fork {
			Fork::Data => &self.data_fork,
			Fork::Resource => &self.resource_fork
		}
	}

	pub fn get_fork_mut(&mut self, fork: Fork) -> &mut Vec<u8> {
		match fork {
			Fork::Data => &mut self.data_fork,
			Fork::Resource => &mut self.resource_fork
		}
	}

	pub fn create<P: AsRef<Path>>(path: P, creator_id: FourCC, type_id: FourCC) -> MacFile {
		let path: &Path = path.as_ref();

		let mode = if xattr::SUPPORTED_PLATFORM {
			FileMode::Native
		} else if type_id == four_cc(*b"TEXT") {
			FileMode::Automatic
		} else {
			FileMode::MacBinary
		};

		MacFile {
			path: path.to_path_buf(),
			mode,
			dirty: true,
			file_info: FileInfo {
				file_type: type_id,
				file_creator: creator_id,
				finder_flags: 0,
				location: (0, 0),
				reserved_field: 0,
				extended_data: [0; 16]
			},
			data_fork: Vec::new(),
			resource_fork: Vec::new()
		}
	}

	pub fn open<P: AsRef<Path>>(path: P) -> Result<MacFile> {
		let path: &Path = path.as_ref();
		let mut file = File::open(path)?;
		let mut data = Vec::new();
		file.read_to_end(&mut data)?;
		let path = path.to_path_buf();

		// Does this file have native metadata?
		if xattr::SUPPORTED_PLATFORM {
			if let Some(metadata) = file.get_xattr("com.apple.FinderInfo")? {
				// Yes, let's do it
				let mut cursor = Cursor::new(&metadata);
				let file_info: FileInfo = cursor.read_be()?;
				let resource_fork = file.get_xattr("com.apple.ResourceFork")?.unwrap_or_default();

				return Ok(MacFile {
					path,
					mode: FileMode::Native,
					dirty: false,
					file_info,
					data_fork: data,
					resource_fork,
				});
			}
		}

		// Is this a MacBinary file?
		if macbinary::probe(&data) {
			let mb = macbinary::unpack(&data)?;

			return Ok(MacFile {
				path,
				mode: FileMode::MacBinary,
				dirty: false,
				file_info: FileInfo {
					file_type: FourCC(mb.type_id),
					file_creator: FourCC(mb.creator_id),
					finder_flags: mb.finder_flags,
					location: mb.location,
					reserved_field: 0,
					extended_data: [0; 16]
				},
				data_fork: mb.data,
				resource_fork: mb.resource
			});
		}

		// It's something else entirely, let's just assume text for now
		Ok(MacFile {
			path,
			mode: FileMode::Automatic,
			dirty: false,
			file_info: FileInfo {
				file_type: four_cc(*b"TEXT"),
				file_creator: four_cc(*b"ttxt"),
				finder_flags: 0,
				location: (0, 0),
				reserved_field: 0,
				extended_data:[0; 16]
			},
			data_fork: data,
			resource_fork: Vec::new()
		})
	}

	fn save(&self) -> Result<()> {
		let mut file = File::create(&self.path)?;

		match self.mode {
			FileMode::Automatic => {
				// simplest mode
				file.write_all(&self.data_fork)?;
			}
			FileMode::MacBinary => {
				unimplemented!();
			}
			FileMode::Native => {
				file.write_all(&self.data_fork)?;
				file.set_xattr("com.apple.FinderInfo", &self.file_info.pack())?;
				if !self.resource_fork.is_empty() {
					file.set_xattr("com.apple.ResourceFork", &self.resource_fork)?;
				}
			}
		}

		Ok(())
	}

	pub fn set_dirty(&mut self) {
		self.dirty = true;
	}

	pub fn save_if_dirty(&mut self) -> Result<()> {
		if self.dirty {
			self.save()?;
			self.dirty = false;
		}
		Ok(())
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Volume {
	// Used on Unix systems
	Root,
	// Used on Windows systems
	Verbatim(OsString),
	VerbatimUNC(OsString, OsString),
	VerbatimDisk(u8)
}

impl Volume {
	fn containing_path(path: &Path) -> Result<Self> {
		assert!(path.is_absolute() && path.has_root());

		match path.components().next().unwrap() {
			std::path::Component::Prefix(prefix) => {
				match prefix.kind() {
					Prefix::Verbatim(a) => Ok(Volume::Verbatim(a.to_owned())),
					Prefix::VerbatimUNC(a, b) => Ok(Volume::VerbatimUNC(a.to_owned(), b.to_owned())),
					Prefix::VerbatimDisk(letter) => Ok(Volume::VerbatimDisk(letter)),
					_ => Err(anyhow!("unexpected prefix in path"))
				}
			}
			std::path::Component::RootDir => Ok(Volume::Root),
			_ => Err(anyhow!("unexpected element at start of path"))
		}
	}

	fn get_root(&self) -> PathBuf {
		match self {
			Volume::Root => PathBuf::from("/"),
			Volume::Verbatim(a) => {
				let mut s = OsString::from(r"\\?\");
				s.push(&a);
				s.push(r"\");
				PathBuf::from(s)
			}
			Volume::VerbatimUNC(a, b) => {
				let mut s = OsString::from(r"\\?\UNC\");
				s.push(&a);
				s.push(r"\");
				s.push(&b);
				s.push(r"\");
				PathBuf::from(s)
			}
			Volume::VerbatimDisk(letter) => {
				let mut s = OsString::from(r"\\?\");
				s.push(std::str::from_utf8(&[*letter]).unwrap());
				s.push(r":\");
				PathBuf::from(s)
			}
		}
	}

	fn get_name(&self) -> Option<String> {
		match self {
			Volume::Root => Some(String::from("Root")),
			Volume::VerbatimDisk(letter) => {
				Some(String::from_utf8(vec![*letter]).unwrap())
			}
			_ => None
		}
	}
}


pub struct FileSystem {
	// should this store a Weak instead of Rc?
	files: HashMap<PathBuf, Rc<RefCell<MacFile>>>,
	nodes: BiHashMap<VolumeAndDir, PathBuf>,
	next_node_id: DirID,
	volume_names: BiHashMap<VolumeRef, String>,
	volumes: BiHashMap<VolumeRef, Volume>,
	default_volume: VolumeRef,
	next_volume_ref: VolumeRef
}

impl FileSystem {
	pub fn new() -> Self {
		FileSystem {
			files: HashMap::new(),
			nodes: BiHashMap::new(),
			next_node_id: 3, // skip 1+2 as these are reserved by Mac OS
			volume_names: BiHashMap::new(),
			volumes: BiHashMap::new(),
			default_volume: -1,
			next_volume_ref: -1
		}
	}

	pub fn get_volume_info_by_drive_number(&self, drive_number: i16) -> Option<(String, i16)> {
		// assume drive numbers are just volume numbers (but positive) for now
		let volume_ref = if drive_number == 0 {
			self.default_volume
		} else {
			-drive_number
		};

		if let Some(name) = self.volume_names.get_by_left(&volume_ref) {
			Some((name.clone(), volume_ref))
		} else {
			None
		}
	}

	fn get_volume_by_name(&mut self, name: &[u8]) -> Result<Volume> {
		let name = mac_roman::decode_string(name, false);

		if let Some(volume_ref) = self.volume_names.get_by_right(name.as_ref()) {
			Ok(self.volumes.get_by_left(volume_ref).unwrap().clone())
		} else {
			// try to guess what this volume is
			if cfg!(windows) && name.len() == 1 && name.chars().next().unwrap().is_ascii_alphabetic() {
				// must be a drive
				let letter = name.chars().next().unwrap().to_ascii_uppercase();
				let volume = Volume::VerbatimDisk(letter as u8);
				let volume_ref = self.next_volume_ref;
				debug!(target: "fs", "Registered volume {volume_ref} to be {volume:?} ({name})");
				self.volume_names.insert(volume_ref, name.into_owned());
				self.volumes.insert(volume_ref, volume.clone());
				self.next_volume_ref -= 1;
				Ok(volume)
			} else {
				Err(anyhow!("could not resolve volume"))
			}
		}
	}

	fn get_volume_by_ref(&self, volume_ref: VolumeRef) -> Result<Volume> {
		if let Some(volume) = self.volumes.get_by_left(&volume_ref) {
			Ok(volume.clone())
		} else {
			Err(anyhow!("unknown volume ref"))
		}
	}

	fn get_volume_ref_for_path(&mut self, path: &Path) -> Result<VolumeRef> {
		let volume = Volume::containing_path(path)?;
		if let Some(volume_ref) = self.volumes.get_by_right(&volume) {
			Ok(*volume_ref)
		} else {
			// register it
			let volume_ref = self.next_volume_ref;
			let name = volume.get_name().unwrap_or_else(|| format!("Volume {volume_ref}"));
			debug!(target: "fs", "Registered volume {volume_ref} to be {volume:?} ({name})");
			self.volume_names.insert(volume_ref, name);
			self.volumes.insert(volume_ref, volume);
			self.next_volume_ref -= 1;
			Ok(volume_ref)
		}
	}

	pub fn get_directory_by_id(&self, volume_ref: VolumeRef, dir_id: DirID) -> Result<PathBuf> {
		if let Some(path) = self.nodes.get_by_left(&(volume_ref, dir_id)) {
			Ok(path.clone())
		} else {
			Err(anyhow!("unknown directory ID"))
		}
	}

	pub fn resolve_path(&mut self, volume_ref: VolumeRef, dir_id: DirID, name: &[u8]) -> Result<PathBuf> {
		// https://web.archive.org/web/20011122070503/http://developer.apple.com/techpubs/mac/Files/Files-91.html#HEADING91-0
		trace!(target: "fs", "resolve_path({volume_ref}, {dir_id}, {name:?})");

		let mut name = name;

		let mut path = if name.contains(&b':') && !name.starts_with(b":") {
			// Full pathname - take the volume off it and continue
			let split_point = name.iter().position(|c| *c == b':').unwrap();
			let (volume_name, rest) = name.split_at(split_point);
			name = rest;
			self.get_volume_by_name(volume_name)?.get_root()
		} else if dir_id == 2 {
			// Root directory
			if volume_ref == 0 {
				std::env::current_dir()?.ancestors().last().unwrap().to_path_buf()
			} else {
				self.get_volume_by_ref(volume_ref)?.get_root()
			}
		} else if dir_id > 2 {
			// Relative pathname from a directory
			self.get_directory_by_id(volume_ref, dir_id)?
		} else {
			// Relative from current directory
			std::env::current_dir()?
		};

		// Apply what's left
		for chunk in name.split(|&c| c == b':') {
			if chunk.is_empty() {
				// should we try to parse "::"? maybe later
				continue;
			}
			if chunk.iter().any(|&c| c == b'/' || c == b'\\' || c < b' ') {
				return Err(anyhow!("invalid character in path element"));
			}
			let chunk = mac_roman::decode_string(chunk, false);
			path.push(chunk.as_ref());
		}

		Ok(path)
	}

	pub fn id_for_dir(&mut self, path: &Path) -> Result<VolumeAndDir> {
		trace!(target: "fs", "id_for_dir({path:?})");

		if let Some(&vad) = self.nodes.get_by_right(path) {
			Ok(vad)
		} else {
			// Throw the lad in
			let volume = self.get_volume_ref_for_path(path)?;
			let dir = if path.parent().is_none() {
				ROOT_DIR_ID
			} else {
				self.next_node_id += 1;
				self.next_node_id - 1
			};
			debug!(target: "fs", "Registered node ({volume}::{dir}) to {path:?}");
			self.nodes.insert((volume, dir), path.to_path_buf());
			Ok((volume, dir))
		}
	}

	pub fn spec(&mut self, path: &Path) -> Result<NodeRef> {
		trace!(target: "fs", "spec({path:?})");

		match path.parent() {
			Some(parent) => {
				let (volume_ref, parent_id) = self.id_for_dir(parent)?;
				let (_, node_id) = self.id_for_dir(path)?;
				let name = path.file_name().unwrap().to_string_lossy();
				let name_enc = mac_roman::encode_string(name.as_ref(), false);
				trace!(target: "fs", " ... => ({volume_ref}, parent={parent_id}, node={node_id}, {name})");
				Ok(NodeRef {
					volume_ref,
					parent_id,
					node_id,
					node_name: name_enc.into_owned()
				})
			}
			None => {
				let volume_ref = self.get_volume_ref_for_path(path)?;
				let name = self.volume_names.get_by_left(&volume_ref).unwrap();
				let name_enc = mac_roman::encode_string(name.as_str(), false).into_owned();
				trace!(target: "fs", " ... => ({volume_ref}, parent-{ROOT_PARENT_DIR_ID}, node={ROOT_DIR_ID}, {name})");
				Ok(NodeRef {
					volume_ref,
					parent_id: ROOT_PARENT_DIR_ID,
					node_id: ROOT_DIR_ID,
					node_name: name_enc
				})
			}
		}
	}

	pub fn create_file(&mut self, path: &Path, creator_id: FourCC, type_id: FourCC) -> Result<()> {
		if self.files.contains_key(path) {
			return Err(anyhow!("file already exists"));
		}

		let mut file = MacFile::create(path, creator_id, type_id);
		file.save_if_dirty()?;

		self.files.insert(path.to_path_buf(), Rc::new(RefCell::new(file)));
		Ok(())
	}

	pub fn get_file(&mut self, path: &Path) -> Result<Rc<RefCell<MacFile>>> {
		if let Some(file) = self.files.get(path) {
			Ok(Rc::clone(file))
		} else {
			let file = MacFile::open(path)?;
			let file = Rc::new(RefCell::new(file));
			self.files.insert(path.to_path_buf(), Rc::clone(&file));
			Ok(file)
		}
	}

	pub fn delete_file(&mut self, path: &Path) -> Result<()> {
		self.files.remove(path);
		std::fs::remove_file(path)?;
		Ok(())
	}
}

pub struct NodeRef {
	pub volume_ref: VolumeRef,
	pub parent_id: DirID,
	pub node_id: DirID,
	pub node_name: Vec<u8>
}

