use std::path::Path;

type ReadingList = Vec<(&'static str, &'static str)>;
type EntriesMap = FxHashMap<Box<str>, Box<[(Box<str>, Box<str>)]>>;
pub type FormsByLemma = FxHashMap<Box<str>, Vec<(Box<str>, Box<str>)>>;
type SynthByLemma = FormsByLemma;
type VerbForms = std::collections::HashMap<String, Vec<String>>;
type LookupCache = std::sync::RwLock<FxHashMap<Box<str>, Box<[(&'static str, &'static str)]>>>;
const LOOKUP_CACHE_CAP: usize = 1 << 20;

#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

const FX_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(FX_SEED);
    }
}

impl std::hash::Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(buf));
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.add(i as u64)
    }

    fn write_u16(&mut self, i: u16) {
        self.add(i as u64)
    }

    fn write_u32(&mut self, i: u32) {
        self.add(i as u64)
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i)
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64)
    }
}

pub type FxBuildHasher = std::hash::BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = std::collections::HashSet<T, FxBuildHasher>;

fn global_intern(owned: String) -> &'static str {
    static POOL: std::sync::OnceLock<std::sync::Mutex<FxHashSet<&'static str>>> =
        std::sync::OnceLock::new();
    let pool = POOL.get_or_init(|| std::sync::Mutex::new(FxHashSet::default()));
    let mut guard = pool.lock().unwrap();
    if let Some(existing) = guard.get(owned.as_str()) {
        return existing;
    }
    let leaked: &'static str = Box::leak(owned.into_boxed_str());
    guard.insert(leaked);
    leaked
}

pub fn intern_string(owned: String) -> &'static str {
    global_intern(owned)
}

struct Fsa5 {
    data: std::sync::Arc<[u8]>,
    arc_start: usize,
    node_data_length: usize,
    gtl: usize,
}

const FSA5_BIT_FINAL_ARC: u8 = 1 << 0;
const FSA5_BIT_LAST_ARC: u8 = 1 << 1;
const FSA5_BIT_TARGET_NEXT: u8 = 1 << 2;
const FSA5_ADDRESS_OFFSET: usize = 1;

impl Fsa5 {
    fn arcs(&self) -> &[u8] {
        &self.data[self.arc_start..]
    }

    fn from_bytes(data: std::sync::Arc<[u8]>) -> Option<Self> {
        if data.len() < 8 || &data[0..4] != b"\\fsa" || data[4] != 0x05 {
            return None;
        }
        let hgtl = *data.get(7)?;
        let node_data_length = ((hgtl >> 4) & 0x0f) as usize;
        let gtl = (hgtl & 0x0f) as usize;
        if gtl == 0 {
            return None;
        }
        Some(Self {
            arc_start: 8,
            node_data_length,
            gtl,
            data,
        })
    }

    fn is_next_set(&self, arc: usize) -> bool {
        self.arcs()
            .get(arc + FSA5_ADDRESS_OFFSET)
            .is_some_and(|f| f & FSA5_BIT_TARGET_NEXT != 0)
    }

    fn skip_arc(&self, offset: usize) -> Option<usize> {
        if self.is_next_set(offset) {
            offset.checked_add(2)
        } else {
            offset.checked_add(1 + self.gtl)
        }
    }

    fn is_arc_last(&self, arc: usize) -> bool {
        self.arcs()
            .get(arc + FSA5_ADDRESS_OFFSET)
            .is_some_and(|f| f & FSA5_BIT_LAST_ARC != 0)
    }

    fn is_arc_final(&self, arc: usize) -> bool {
        self.arcs()
            .get(arc + FSA5_ADDRESS_OFFSET)
            .is_some_and(|f| f & FSA5_BIT_FINAL_ARC != 0)
    }

    fn destination(&self, arc: usize) -> Option<usize> {
        if self.is_next_set(arc) {
            self.skip_arc(arc)
        } else {
            let mut value = 0usize;
            for i in (0..self.gtl).rev() {
                let b = *self.arcs().get(arc + FSA5_ADDRESS_OFFSET + i)?;
                value = (value << 8) | b as usize;
            }
            Some(value >> 3)
        }
    }

    fn root_node(&self) -> Option<usize> {
        let first = self.node_data_length;
        let epsilon = self.skip_arc(first)?;
        let eps_first = epsilon + self.node_data_length;
        self.destination(eps_first)
    }

    fn walk(&self, node: usize, sequence: &mut Vec<u8>, out: &mut dyn FnMut(&[u8])) {
        let mut arc = Some(self.node_data_length + node);
        while let Some(current) = arc {
            let Some(&label) = self.arcs().get(current) else {
                break;
            };
            sequence.push(label);
            if self.is_arc_final(current) {
                out(sequence);
            }
            if let Some(dest) = self.destination(current)
                && dest != 0
            {
                self.walk(dest, sequence, out);
            }
            sequence.pop();
            if self.is_arc_last(current) {
                break;
            }
            arc = self.skip_arc(current);
        }
    }

    fn seek(&self, node: usize, key: &[u8]) -> Option<usize> {
        let mut node = node;
        for &byte in key {
            let mut arc = Some(self.node_data_length + node);
            let mut next = None;
            while let Some(current) = arc {
                let Some(&label) = self.arcs().get(current) else {
                    break;
                };
                if label == byte {
                    next = self.destination(current);
                    break;
                }
                if self.is_arc_last(current) {
                    break;
                }
                arc = self.skip_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return None,
            };
        }
        Some(node)
    }

    fn contains_word(&self, root: usize, key: &[u8], separator: u8) -> bool {
        let mut node = root;
        let mut last_final = false;
        for (i, &byte) in key.iter().enumerate() {
            let mut arc = Some(self.node_data_length + node);
            let mut next = None;
            while let Some(current) = arc {
                let Some(&label) = self.arcs().get(current) else {
                    break;
                };
                if label == byte {
                    last_final = self.is_arc_final(current);
                    next = self.destination(current);
                    break;
                }
                if self.is_arc_last(current) {
                    break;
                }
                arc = self.skip_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return last_final && i + 1 == key.len(),
            };
        }
        if last_final {
            return true;
        }
        let mut arc = Some(self.node_data_length + node);
        while let Some(current) = arc {
            let Some(&label) = self.arcs().get(current) else {
                break;
            };
            if label == separator {
                return true;
            }
            if self.is_arc_last(current) {
                break;
            }
            arc = self.skip_arc(current);
        }
        false
    }
}

struct Cfsa1 {
    data: std::sync::Arc<[u8]>,
    arc_start: usize,
    label_start: usize,
    label_end: usize,
    node_data_length: usize,
    gtl: usize,
}

const CFSA1_BIT_FINAL_ARC: u8 = 1 << 0;
const CFSA1_BIT_LAST_ARC: u8 = 1 << 1;
const CFSA1_BIT_TARGET_NEXT: u8 = 1 << 2;

impl Cfsa1 {
    fn arcs(&self) -> &[u8] {
        &self.data[self.arc_start..]
    }

    fn label_mapping(&self) -> &[u8] {
        &self.data[self.label_start..self.label_end]
    }

    fn from_bytes(data: std::sync::Arc<[u8]>) -> Option<Self> {
        if data.len() < 44 || &data[0..4] != b"\\fsa" || data[4] != 0xc5 {
            return None;
        }
        let hgtl = *data.get(7)?;
        let node_data_length = if hgtl & 0xf0 != 0 {
            ((hgtl >> 4) & 0x0f) as usize
        } else {
            0
        };
        let gtl = (hgtl & 0x0f) as usize;
        if gtl == 0 {
            return None;
        }
        Some(Self {
            arc_start: 40,
            label_start: 8,
            label_end: 40,
            node_data_length,
            gtl,
            data,
        })
    }

    fn is_next_set(&self, arc: usize) -> bool {
        self.arcs()
            .get(arc)
            .is_some_and(|f| f & CFSA1_BIT_TARGET_NEXT != 0)
    }

    fn is_label_compressed(&self, arc: usize) -> bool {
        self.is_next_set(arc) && (self.arcs()[arc] >> 3) & 0x1f != 0
    }

    fn arc_label(&self, arc: usize) -> u8 {
        if self.is_label_compressed(arc) {
            self.label_mapping()[(self.arcs()[arc] >> 3) as usize & 0x1f]
        } else {
            self.arcs()[arc + 1]
        }
    }

    fn skip_arc(&self, arc: usize) -> Option<usize> {
        if self.is_next_set(arc) {
            if self.is_label_compressed(arc) {
                arc.checked_add(1)
            } else {
                arc.checked_add(2)
            }
        } else {
            arc.checked_add(1 + self.gtl)
        }
    }

    fn destination(&self, arc: usize) -> Option<usize> {
        if self.is_next_set(arc) {
            self.skip_arc(arc)
        } else {
            let mut value = 0usize;
            for i in (1..self.gtl).rev() {
                let b = *self.arcs().get(arc + 1 + i)?;
                value = (value << 8) | b as usize;
            }
            let flag = *self.arcs().get(arc)? as usize;
            Some(((value << 8) | flag) >> 3)
        }
    }

    fn root_node(&self) -> Option<usize> {
        let epsilon = self.skip_arc(self.node_data_length)?;
        self.destination(epsilon + self.node_data_length)
    }

    fn walk(&self, node: usize, sequence: &mut Vec<u8>, out: &mut dyn FnMut(&[u8])) {
        let mut arc = Some(self.node_data_length + node);
        while let Some(current) = arc {
            let Some(&flag) = self.arcs().get(current) else {
                break;
            };
            let label = self.arc_label(current);
            sequence.push(label);
            if flag & CFSA1_BIT_FINAL_ARC != 0 {
                out(sequence);
            }
            if let Some(dest) = self.destination(current)
                && dest != 0
            {
                self.walk(dest, sequence, out);
            }
            sequence.pop();
            if flag & CFSA1_BIT_LAST_ARC != 0 {
                break;
            }
            arc = self.skip_arc(current);
        }
    }

    fn seek(&self, node: usize, key: &[u8]) -> Option<usize> {
        let mut node = node;
        for &byte in key {
            let mut arc = Some(self.node_data_length + node);
            let mut next = None;
            while let Some(current) = arc {
                let Some(&flag) = self.arcs().get(current) else {
                    break;
                };
                if self.arc_label(current) == byte {
                    next = self.destination(current);
                    break;
                }
                if flag & CFSA1_BIT_LAST_ARC != 0 {
                    break;
                }
                arc = self.skip_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return None,
            };
        }
        Some(node)
    }

    fn contains_word(&self, root: usize, key: &[u8], separator: u8) -> bool {
        let mut node = root;
        let mut last_final = false;
        for (i, &byte) in key.iter().enumerate() {
            let mut arc = Some(self.node_data_length + node);
            let mut next = None;
            while let Some(current) = arc {
                let Some(&flag) = self.arcs().get(current) else {
                    break;
                };
                if self.arc_label(current) == byte {
                    last_final = flag & CFSA1_BIT_FINAL_ARC != 0;
                    next = self.destination(current);
                    break;
                }
                if flag & CFSA1_BIT_LAST_ARC != 0 {
                    break;
                }
                arc = self.skip_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return last_final && i + 1 == key.len(),
            };
        }
        if last_final {
            return true;
        }
        let mut arc = Some(self.node_data_length + node);
        while let Some(current) = arc {
            let Some(&flag) = self.arcs().get(current) else {
                break;
            };
            if self.arc_label(current) == separator {
                return true;
            }
            if flag & CFSA1_BIT_LAST_ARC != 0 {
                break;
            }
            arc = self.skip_arc(current);
        }
        false
    }
}

struct Cfsa2 {
    data: std::sync::Arc<[u8]>,
    arc_start: usize,
    label_start: usize,
    label_end: usize,
    has_numbers: bool,
}

fn read_vint(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let mut shift = 0;
    let mut value = 0usize;
    let mut pos = offset;
    loop {
        let byte = *data.get(pos)?;
        pos += 1;
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Some((value, pos));
        }
        shift += 7;
    }
}

impl Cfsa2 {
    fn arcs(&self) -> &[u8] {
        &self.data[self.arc_start..]
    }

    fn label_mapping(&self) -> &[u8] {
        &self.data[self.label_start..self.label_end]
    }

    fn from_bytes(data: std::sync::Arc<[u8]>) -> Option<Self> {
        if data.len() < 8 || &data[0..4] != b"\\fsa" || data[4] != 0xc6 {
            return None;
        }
        let mut pos = 5;
        let flag_bits = u16::from_be_bytes([*data.get(pos)?, *data.get(pos + 1)?]);
        pos += 2;
        let has_numbers = flag_bits & (1 << 8) != 0;
        let label_mapping_size = *data.get(pos)? as usize;
        pos += 1;
        let label_start = pos;
        pos += label_mapping_size;
        Some(Self {
            arc_start: pos,
            label_start,
            label_end: pos,
            has_numbers,
            data,
        })
    }

    fn first_arc(&self, node: usize) -> usize {
        if self.has_numbers {
            read_vint(self.arcs(), node).map_or(node, |(_, next)| next)
        } else {
            node
        }
    }

    fn arc_label(&self, arc: usize) -> u8 {
        let index = self.arcs()[arc] & 0x1f;
        if index > 0 {
            self.label_mapping()[index as usize]
        } else {
            self.arcs()[arc + 1]
        }
    }

    fn is_arc_last(&self, arc: usize) -> bool {
        self.arcs()[arc] & 0x40 != 0
    }

    fn is_next_set(&self, arc: usize) -> bool {
        self.arcs()[arc] & 0x80 != 0
    }

    fn skip_arc(&self, arc: usize) -> usize {
        let mut offset = arc + 1;
        if self.arcs()[arc] & 0x1f == 0 {
            offset += 1;
        }
        if !self.is_next_set(arc) {
            while self.arcs().get(offset).is_some_and(|b| b & 0x80 != 0) {
                offset += 1;
            }
            offset += 1;
        }
        offset
    }

    fn next_arc(&self, arc: usize) -> Option<usize> {
        if self.is_arc_last(arc) {
            None
        } else {
            Some(self.skip_arc(arc))
        }
    }

    fn destination(&self, arc: usize) -> usize {
        if self.is_next_set(arc) {
            let mut current = arc;
            while !self.is_arc_last(current) {
                current = self.next_arc(current).unwrap_or(current);
            }
            self.skip_arc(current)
        } else {
            let start = arc + if self.arcs()[arc] & 0x1f == 0 { 2 } else { 1 };
            read_vint(self.arcs(), start).map_or(0, |(value, _)| value)
        }
    }

    fn root(&self) -> usize {
        self.destination(self.first_arc(0))
    }

    fn is_arc_final(&self, arc: usize) -> bool {
        self.arcs()[arc] & 0x20 != 0
    }

    fn seek(&self, node: usize, key: &[u8]) -> Option<usize> {
        let mut node = node;
        for &byte in key {
            let mut arc = Some(self.first_arc(node));
            let mut next = None;
            while let Some(current) = arc {
                if self.arc_label(current) == byte {
                    next = Some(self.destination(current));
                    break;
                }
                arc = self.next_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return None,
            };
        }
        Some(node)
    }

    fn contains_word(&self, root: usize, key: &[u8], separator: u8) -> bool {
        let mut node = root;
        let mut last_final = false;
        for (i, &byte) in key.iter().enumerate() {
            let mut arc = Some(self.first_arc(node));
            let mut next = None;
            while let Some(current) = arc {
                if self.arc_label(current) == byte {
                    last_final = self.is_arc_final(current);
                    next = Some(self.destination(current));
                    break;
                }
                arc = self.next_arc(current);
            }
            node = match next {
                Some(dest) if dest != 0 => dest,
                _ => return last_final && i + 1 == key.len(),
            };
        }
        if last_final {
            return true;
        }
        let mut arc = Some(self.first_arc(node));
        while let Some(current) = arc {
            if self.arc_label(current) == separator {
                return true;
            }
            arc = self.next_arc(current);
        }
        false
    }

    fn walk(&self, node: usize, sequence: &mut Vec<u8>, out: &mut dyn FnMut(&[u8])) {
        let mut arc = Some(self.first_arc(node));
        while let Some(current) = arc {
            let label = self.arc_label(current);
            sequence.push(label);
            if self.is_arc_final(current) {
                out(sequence);
            }
            let dest = self.destination(current);
            if dest != 0 {
                self.walk(dest, sequence, out);
            }
            sequence.pop();
            arc = self.next_arc(current);
        }
    }
}

enum Fsa {
    V5(Fsa5),
    C1(Cfsa1),
    C2(Cfsa2),
}

impl Fsa {
    fn from_bytes(data: std::sync::Arc<[u8]>) -> Option<Self> {
        if let Some(f) = Cfsa2::from_bytes(data.clone()) {
            return Some(Self::C2(f));
        }
        if let Some(f) = Fsa5::from_bytes(data.clone()) {
            return Some(Self::V5(f));
        }
        if let Some(f) = Cfsa1::from_bytes(data) {
            return Some(Self::C1(f));
        }
        None
    }

    fn for_each_sequence(&self, out: &mut dyn FnMut(&[u8])) {
        let mut sequence: Vec<u8> = Vec::new();
        match self {
            Self::V5(f) => {
                if let Some(root) = f.root_node() {
                    f.walk(root, &mut sequence, out);
                }
            }
            Self::C1(f) => {
                if let Some(root) = f.root_node() {
                    f.walk(root, &mut sequence, out);
                }
            }
            Self::C2(f) => f.walk(f.root(), &mut sequence, out),
        }
    }

    fn seek(&self, key: &[u8]) -> Option<usize> {
        match self {
            Self::V5(f) => f.root_node().and_then(|root| f.seek(root, key)),
            Self::C1(f) => f.root_node().and_then(|root| f.seek(root, key)),
            Self::C2(f) => f.seek(f.root(), key),
        }
    }

    fn contains_word(&self, key: &[u8], separator: u8) -> bool {
        match self {
            Self::V5(f) => f
                .root_node()
                .is_some_and(|root| f.contains_word(root, key, separator)),
            Self::C1(f) => f
                .root_node()
                .is_some_and(|root| f.contains_word(root, key, separator)),
            Self::C2(f) => f.contains_word(f.root(), key, separator),
        }
    }

    fn has_sequence(&self) -> bool {
        match self {
            Self::V5(f) => f
                .root_node()
                .is_some_and(|root| root != 0 && f.arcs().get(f.node_data_length + root).is_some()),
            Self::C1(f) => f
                .root_node()
                .is_some_and(|root| root != 0 && f.arcs().get(f.node_data_length + root).is_some()),
            Self::C2(f) => f.root() != 0 && f.arcs().get(f.root()).is_some(),
        }
    }

    fn separator_suffixes(&self, node: usize, separator: u8, out: &mut dyn FnMut(&[u8])) {
        match self {
            Self::V5(f) => {
                let mut arc = Some(f.node_data_length + node);
                while let Some(current) = arc {
                    let Some(&label) = f.arcs().get(current) else {
                        break;
                    };
                    if label == separator
                        && let Some(dest) = f.destination(current)
                        && dest != 0
                    {
                        let mut sequence: Vec<u8> = Vec::new();
                        f.walk(dest, &mut sequence, out);
                    }
                    if f.is_arc_last(current) {
                        break;
                    }
                    arc = f.skip_arc(current);
                }
            }
            Self::C1(f) => {
                let mut arc = Some(f.node_data_length + node);
                while let Some(current) = arc {
                    let Some(&flag) = f.arcs().get(current) else {
                        break;
                    };
                    if f.arc_label(current) == separator
                        && let Some(dest) = f.destination(current)
                        && dest != 0
                    {
                        let mut sequence: Vec<u8> = Vec::new();
                        f.walk(dest, &mut sequence, out);
                    }
                    if flag & CFSA1_BIT_LAST_ARC != 0 {
                        break;
                    }
                    arc = f.skip_arc(current);
                }
            }
            Self::C2(f) => {
                let mut arc = Some(f.first_arc(node));
                while let Some(current) = arc {
                    if f.arc_label(current) == separator {
                        let dest = f.destination(current);
                        if dest != 0 {
                            let mut sequence: Vec<u8> = Vec::new();
                            f.walk(dest, &mut sequence, out);
                        }
                    }
                    arc = f.next_arc(current);
                }
            }
        }
    }
}

fn trim_frequency_tag(frequency_included: bool, tag: &str) -> &str {
    if !frequency_included {
        return tag;
    }
    match tag.char_indices().next_back() {
        Some((end, _)) if end > 0 => &tag[..end],
        _ => tag,
    }
}

fn decode_suffix(word: &[u8], encoded: &[u8]) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return None;
    }
    let trim = encoded[0].wrapping_sub(b'A') as usize;
    let trim = if trim == 255 { word.len() } else { trim };
    if trim > word.len() {
        return None;
    }
    Some([&word[..word.len() - trim], &encoded[1..]].concat())
}

fn decode_prefix_and_suffix(word: &[u8], encoded: &[u8]) -> Option<Vec<u8>> {
    if encoded.len() < 2 {
        return None;
    }
    let trim_start = encoded[0].wrapping_sub(b'A') as usize;
    let trim_end = encoded[1].wrapping_sub(b'A') as usize;
    let (trim_start, trim_end) = if trim_start == 255 || trim_end == 255 {
        (word.len(), 0)
    } else {
        (trim_start, trim_end)
    };
    if trim_start + trim_end > word.len() {
        return None;
    }
    Some([&word[trim_start..word.len() - trim_end], &encoded[2..]].concat())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LemmaEncoder {
    Raw,
    Suffix,
    PrefixAndSuffix,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DictEncoding {
    Utf8,
    Koi8R,
    Windows1251,
    Latin1,
    Latin15,
}

impl DictEncoding {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "koi8-r" => Self::Koi8R,
            "windows-1251" | "cp1251" => Self::Windows1251,
            "iso-8859-1" | "iso8859-1" => Self::Latin1,
            "iso-8859-15" | "iso8859-15" => Self::Latin15,
            _ => Self::Utf8,
        }
    }

    fn decode(&self, bytes: &[u8]) -> String {
        match self {
            Self::Koi8R => encoding_rs::KOI8_R.decode(bytes).0.into_owned(),
            Self::Windows1251 => encoding_rs::WINDOWS_1251.decode(bytes).0.into_owned(),
            Self::Latin1 => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
            Self::Latin15 => {
                let latin = encoding_rs::WINDOWS_1252.decode(bytes).0;
                latin
                    .chars()
                    .map(|c| match c {
                        '¤' => '€',
                        '¦' => 'Š',
                        '¨' => 'š',
                        '´' => 'Ž',
                        '¸' => 'ž',
                        '¼' => 'Œ',
                        '½' => 'œ',
                        '¾' => 'Ÿ',
                        other => other,
                    })
                    .collect()
            }
            Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        }
    }

    fn encode(&self, text: &str) -> Option<Vec<u8>> {
        match self {
            Self::Utf8 => Some(text.as_bytes().to_vec()),
            Self::Koi8R => encode_single_byte(text, encoding_rs::KOI8_R),
            Self::Windows1251 => encode_single_byte(text, encoding_rs::WINDOWS_1251),
            Self::Latin1 => encode_single_byte(text, encoding_rs::WINDOWS_1252),
            Self::Latin15 => {
                let mapped: String = text
                    .chars()
                    .map(|c| match c {
                        '€' => '¤',
                        'Š' => '¦',
                        'š' => '¨',
                        'Ž' => '´',
                        'ž' => '¸',
                        'Œ' => '¼',
                        'œ' => '½',
                        'Ÿ' => '¾',
                        other => other,
                    })
                    .collect();
                encode_single_byte(&mapped, encoding_rs::WINDOWS_1252)
            }
        }
    }
}

fn encode_single_byte(text: &str, encoding: &'static encoding_rs::Encoding) -> Option<Vec<u8>> {
    let (bytes, _, had_errors) = encoding.encode(text);
    if had_errors {
        return None;
    }
    Some(bytes.into_owned())
}

fn decode_bytes(encoding: &str, bytes: &[u8]) -> String {
    DictEncoding::parse(encoding).decode(bytes)
}

fn info_value(info: &str, key: &str) -> Option<String> {
    info.lines()
        .find_map(|l| l.trim().strip_prefix(key))
        .map(str::trim)
        .map(str::to_string)
}

pub fn load_fsa_word_list(dict_path: &Path, info_path: &Path) -> Option<Vec<String>> {
    let data: std::sync::Arc<[u8]> =
        std::sync::Arc::from(std::fs::read(dict_path).ok()?.into_boxed_slice());
    let info = std::fs::read_to_string(info_path).ok()?;
    let separator = info_value(&info, "fsa.dict.separator=")?.chars().next()?;
    let encoding = info_value(&info, "fsa.dict.encoding=").unwrap_or_else(|| "utf8".into());
    let sep = separator as u8;
    let fsa = Fsa::from_bytes(data)?;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    fsa.for_each_sequence(&mut |sequence: &[u8]| {
        let word = sequence.split(|b| *b == sep).next().unwrap_or(&[]);
        if word.is_empty() {
            return;
        }
        seen.insert(decode_bytes(&encoding, word));
    });
    Some(seen.into_iter().collect())
}

pub struct FsaWordSet {
    fsa: Fsa,
    separator: u8,
    encoding: DictEncoding,
    word_count: std::sync::OnceLock<u64>,
}

impl FsaWordSet {
    pub fn load(dict_path: &Path, info_path: &Path) -> Option<Self> {
        let data: std::sync::Arc<[u8]> =
            std::sync::Arc::from(std::fs::read(dict_path).ok()?.into_boxed_slice());
        let info = std::fs::read_to_string(info_path).ok()?;
        let separator = info_value(&info, "fsa.dict.separator=")?.chars().next()?;
        let encoding = DictEncoding::parse(
            &info_value(&info, "fsa.dict.encoding=").unwrap_or_else(|| "utf8".into()),
        );
        let fsa = Fsa::from_bytes(data)?;
        Some(Self {
            fsa,
            separator: separator as u8,
            encoding,
            word_count: std::sync::OnceLock::new(),
        })
    }

    pub fn contains(&self, word: &str) -> bool {
        if word.is_empty() {
            return false;
        }
        let Some(bytes) = self.encoding.encode(word) else {
            return false;
        };
        if bytes.contains(&self.separator) {
            return false;
        }
        self.fsa.contains_word(&bytes, self.separator)
    }

    pub fn is_empty(&self) -> bool {
        !self.fsa.has_sequence()
    }

    pub fn word_count(&self) -> u64 {
        *self.word_count.get_or_init(|| {
            let separator = self.separator;
            let mut last: Vec<u8> = Vec::new();
            let mut count: u64 = 0;
            self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
                let word = sequence.split(|b| *b == separator).next().unwrap_or(&[]);
                if word.is_empty() || word == last.as_slice() {
                    return;
                }
                count += 1;
                last.clear();
                last.extend_from_slice(word);
            });
            count
        })
    }

    pub fn for_each_word(&self, mut f: impl FnMut(&str)) {
        let separator = self.separator;
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let word = sequence.split(|b| *b == separator).next().unwrap_or(&[]);
            if word.is_empty() {
                return;
            }
            f(&self.encoding.decode(word));
        });
    }

    pub fn word_list(&self) -> Vec<String> {
        let separator = self.separator;
        let mut words: Vec<String> = Vec::new();
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let word = sequence.split(|b| *b == separator).next().unwrap_or(&[]);
            if word.is_empty() {
                return;
            }
            let decoded = self.encoding.decode(word);
            if words.last().map(String::as_str) != Some(decoded.as_str()) {
                words.push(decoded);
            }
        });
        words
    }
}

struct FsaDict {
    fsa: Fsa,
    separator: u8,
    encoder: LemmaEncoder,
    encoding: DictEncoding,
    frequency_included: bool,
    cache: LookupCache,
}

struct Materialized {
    entries: std::sync::OnceLock<EntriesMap>,
    synth_by_lemma: std::sync::OnceLock<Option<SynthByLemma>>,
    forms_by_lemma: std::sync::OnceLock<FormsByLemma>,
    verb_forms: std::sync::OnceLock<VerbForms>,
}

impl Materialized {
    fn empty() -> Self {
        Self {
            entries: std::sync::OnceLock::new(),
            synth_by_lemma: std::sync::OnceLock::new(),
            forms_by_lemma: std::sync::OnceLock::new(),
            verb_forms: std::sync::OnceLock::new(),
        }
    }
}

enum Source {
    Fsa {
        dict: FsaDict,
        materialized: Materialized,
    },
    Decoded {
        materialized: Materialized,
        cache: LookupCache,
    },
}

pub struct MorfologikDictionary {
    source: Source,
}

impl FsaDict {
    fn decode_lemma(&self, word_bytes: &[u8], second: &[u8]) -> Option<Vec<u8>> {
        match self.encoder {
            LemmaEncoder::Raw => Some(second.to_vec()),
            LemmaEncoder::Suffix => decode_suffix(word_bytes, second),
            LemmaEncoder::PrefixAndSuffix => decode_prefix_and_suffix(word_bytes, second),
        }
    }

    fn lookup_uncached(&self, word: &str) -> ReadingList {
        let Some(word_bytes) = self.encoding.encode(word) else {
            return Vec::new();
        };
        let Some(node) = self.fsa.seek(&word_bytes) else {
            return Vec::new();
        };
        let mut readings: ReadingList = Vec::new();
        {
            self.fsa
                .separator_suffixes(node, self.separator, &mut |suffix: &[u8]| {
                    let mut fields = suffix.split(|b| *b == self.separator);
                    let (Some(second), Some(tag_raw)) = (fields.next(), fields.next()) else {
                        return;
                    };
                    let Some(lemma_bytes) = self.decode_lemma(&word_bytes, second) else {
                        return;
                    };
                    let tag = global_intern(
                        trim_frequency_tag(self.frequency_included, &self.encoding.decode(tag_raw))
                            .to_string(),
                    );
                    let lemma = global_intern(self.encoding.decode(&lemma_bytes));
                    readings.push((lemma, tag));
                });
        }
        readings
    }

    fn build_entries(&self) -> EntriesMap {
        let mut entries: FxHashMap<Box<str>, Vec<(Box<str>, Box<str>)>> = FxHashMap::default();
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let mut fields = sequence.split(|b| *b == self.separator);
            let (word_raw, second, tag_raw) = match (fields.next(), fields.next(), fields.next()) {
                (Some(w), Some(s), Some(t)) => (w, s, t),
                _ => return,
            };
            let Some(lemma_bytes) = self.decode_lemma(word_raw, second) else {
                return;
            };
            let word = self.encoding.decode(word_raw);
            let tag = trim_frequency_tag(self.frequency_included, &self.encoding.decode(tag_raw))
                .to_string();
            let lemma = self.encoding.decode(&lemma_bytes);
            entries
                .entry(Box::from(word.as_str()))
                .or_default()
                .push((Box::from(lemma.as_str()), Box::from(tag.as_str())));
        });
        entries
            .into_iter()
            .map(|(word, readings)| (word, readings.into_boxed_slice()))
            .collect()
    }

    fn build_synth(&self) -> Option<SynthByLemma> {
        let mut synth_map: SynthByLemma = FxHashMap::default();
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let mut fields = sequence.split(|b| *b == self.separator);
            let (word_raw, second) = match (fields.next(), fields.next(), fields.next()) {
                (Some(w), Some(s), None) => (w, s),
                _ => return,
            };
            let Some(lemma_bytes) = self.decode_lemma(word_raw, second) else {
                return;
            };
            let word = self.encoding.decode(word_raw);
            let Some((lemma, tag)) = word.split_once('|') else {
                return;
            };
            let form = self.encoding.decode(&lemma_bytes);
            synth_map
                .entry(Box::from(lemma))
                .or_default()
                .push((Box::from(tag), Box::from(form)));
        });
        if synth_map.is_empty() {
            None
        } else {
            Some(synth_map)
        }
    }

    fn build_forms(&self) -> FormsByLemma {
        let mut forms: FormsByLemma = FxHashMap::default();
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let mut fields = sequence.split(|b| *b == self.separator);
            let (word_raw, second, tag_raw) = match (fields.next(), fields.next(), fields.next()) {
                (Some(w), Some(s), Some(t)) => (w, s, t),
                _ => return,
            };
            let Some(lemma_bytes) = self.decode_lemma(word_raw, second) else {
                return;
            };
            let word = self.encoding.decode(word_raw);
            let tag = trim_frequency_tag(self.frequency_included, &self.encoding.decode(tag_raw))
                .to_string();
            let lemma = self.encoding.decode(&lemma_bytes);
            forms
                .entry(Box::from(lemma.as_str()))
                .or_default()
                .push((Box::from(tag.as_str()), Box::from(word.as_str())));
        });
        forms
    }

    fn build_verb_forms(&self) -> VerbForms {
        let mut verb_forms: VerbForms = std::collections::HashMap::new();
        self.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let mut fields = sequence.split(|b| *b == self.separator);
            let (word_raw, second, tag_raw) = match (fields.next(), fields.next(), fields.next()) {
                (Some(w), Some(s), Some(t)) => (w, s, t),
                _ => return,
            };
            let Some(lemma_bytes) = self.decode_lemma(word_raw, second) else {
                return;
            };
            let tag = trim_frequency_tag(self.frequency_included, &self.encoding.decode(tag_raw))
                .to_string();
            if !tag.starts_with("VER:") {
                return;
            }
            let word = self.encoding.decode(word_raw);
            let lemma = self.encoding.decode(&lemma_bytes);
            let entry = verb_forms.entry(lemma).or_default();
            if !entry.iter().any(|w| *w == word) {
                entry.push(word);
            }
        });
        verb_forms
    }
}

impl MorfologikDictionary {
    pub fn load(dict_path: &Path, info_path: &Path) -> Option<Self> {
        let data: std::sync::Arc<[u8]> =
            std::sync::Arc::from(std::fs::read(dict_path).ok()?.into_boxed_slice());
        let info = std::fs::read_to_string(info_path).ok()?;
        let separator = info_value(&info, "fsa.dict.separator=")?.chars().next()?;
        let encoder = info
            .lines()
            .find_map(|l| l.trim().strip_prefix("fsa.dict.encoder="))
            .map(|v| match v.trim().to_ascii_lowercase().as_str() {
                "suffix" => LemmaEncoder::Suffix,
                "prefix" => LemmaEncoder::PrefixAndSuffix,
                _ => LemmaEncoder::Raw,
            })
            .unwrap_or(LemmaEncoder::Raw);
        let encoding = DictEncoding::parse(
            &info_value(&info, "fsa.dict.encoding=").unwrap_or_else(|| "utf8".into()),
        );
        let frequency_included = info_value(&info, "fsa.dict.frequency-included=")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let fsa = Fsa::from_bytes(data)?;
        Some(Self {
            source: Source::Fsa {
                dict: FsaDict {
                    fsa,
                    separator: separator as u8,
                    encoder,
                    encoding,
                    frequency_included,
                    cache: std::sync::RwLock::new(FxHashMap::default()),
                },
                materialized: Materialized::empty(),
            },
        })
    }

    pub fn from_decoded_text(data: &str) -> Self {
        let mut map: FxHashMap<Box<str>, Vec<(Box<str>, Box<str>)>> = FxHashMap::default();
        let mut forms: FormsByLemma = FxHashMap::default();
        let mut verb_forms: VerbForms = std::collections::HashMap::new();
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(form), Some(lemma), Some(tag)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let tag = tag.trim();
            if form.is_empty() || tag.is_empty() {
                continue;
            }
            let lemma: &str = if lemma.is_empty() { form } else { lemma };
            map.entry(Box::from(form))
                .or_default()
                .push((Box::from(lemma), Box::from(tag)));
            forms
                .entry(Box::from(lemma))
                .or_default()
                .push((Box::from(tag), Box::from(form)));
            if tag.starts_with("VER:") {
                let entry = verb_forms.entry(lemma.to_string()).or_default();
                if !entry.iter().any(|w| w == form) {
                    entry.push(form.to_string());
                }
            }
        }
        Self {
            source: Source::Decoded {
                materialized: Materialized {
                    entries: std::sync::OnceLock::from(
                        map.into_iter()
                            .map(|(word, readings)| (word, readings.into_boxed_slice()))
                            .collect::<EntriesMap>(),
                    ),
                    synth_by_lemma: std::sync::OnceLock::from(None),
                    forms_by_lemma: std::sync::OnceLock::from(forms),
                    verb_forms: std::sync::OnceLock::from(verb_forms),
                },
                cache: std::sync::RwLock::new(FxHashMap::default()),
            },
        }
    }

    fn materialized(&self) -> &Materialized {
        match &self.source {
            Source::Fsa { materialized, .. } | Source::Decoded { materialized, .. } => materialized,
        }
    }

    fn entries_map(&self) -> &EntriesMap {
        let m = self.materialized();
        match &self.source {
            Source::Fsa { dict, materialized } => {
                materialized.entries.get_or_init(|| dict.build_entries())
            }
            Source::Decoded { .. } => m.entries.get_or_init(EntriesMap::default),
        }
    }

    pub fn synth_forms_by_lemma(&self) -> Option<&SynthByLemma> {
        let m = self.materialized();
        match &self.source {
            Source::Fsa { dict, materialized } => materialized
                .synth_by_lemma
                .get_or_init(|| dict.build_synth())
                .as_ref(),
            Source::Decoded { .. } => m.synth_by_lemma.get_or_init(|| None).as_ref(),
        }
    }

    pub fn word_list(&self) -> Vec<String> {
        self.entries_map().keys().map(|k| k.to_string()).collect()
    }

    pub fn has_entries(&self) -> bool {
        match &self.source {
            Source::Fsa { dict, .. } => dict.fsa.has_sequence(),
            Source::Decoded { materialized, .. } => {
                !materialized.entries.get().is_some_and(|e| e.is_empty())
            }
        }
    }

    pub fn fsa_word_list(&self) -> Option<Vec<String>> {
        let Source::Fsa { dict, .. } = &self.source else {
            return None;
        };
        let separator = dict.separator;
        let mut words: Vec<String> = Vec::new();
        dict.fsa.for_each_sequence(&mut |sequence: &[u8]| {
            let word = sequence.split(|b| *b == separator).next().unwrap_or(&[]);
            if word.is_empty() {
                return;
            }
            let decoded = dict.encoding.decode(word);
            if words.last().map(String::as_str) != Some(decoded.as_str()) {
                words.push(decoded);
            }
        });
        Some(words)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries_map().keys().map(|k| &**k)
    }

    pub fn forms_by_lemma(&self) -> &FormsByLemma {
        match &self.source {
            Source::Fsa { dict, materialized } => materialized
                .forms_by_lemma
                .get_or_init(|| dict.build_forms()),
            Source::Decoded { materialized, .. } => materialized
                .forms_by_lemma
                .get_or_init(FormsByLemma::default),
        }
    }

    pub fn verb_forms_by_lemma(&self) -> &VerbForms {
        match &self.source {
            Source::Fsa { dict, materialized } => materialized
                .verb_forms
                .get_or_init(|| dict.build_verb_forms()),
            Source::Decoded { materialized, .. } => {
                materialized.verb_forms.get_or_init(VerbForms::default)
            }
        }
    }

    pub fn lookup(&self, word: &str) -> Vec<(&'static str, &'static str)> {
        let cache = match &self.source {
            Source::Fsa { dict, .. } => &dict.cache,
            Source::Decoded { cache, .. } => cache,
        };
        if let Ok(guard) = cache.read()
            && let Some(hit) = guard.get(word)
        {
            return hit.to_vec();
        }
        let miss: ReadingList = match &self.source {
            Source::Fsa { dict, .. } => dict.lookup_uncached(word),
            Source::Decoded { .. } => self
                .entries_map()
                .get(word)
                .map(|v| {
                    v.iter()
                        .map(|(lemma, tag)| {
                            (
                                global_intern(lemma.to_string()),
                                global_intern(tag.to_string()),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        if let Ok(mut guard) = cache.write() {
            if guard.len() >= LOOKUP_CACHE_CAP {
                guard.clear();
            }
            guard
                .entry(Box::from(word))
                .or_insert_with(|| miss.clone().into_boxed_slice());
        }
        miss
    }

    pub fn len(&self) -> usize {
        self.entries_map().len()
    }

    pub fn is_empty(&self) -> bool {
        match &self.source {
            Source::Fsa { dict, materialized } => match materialized.entries.get() {
                Some(entries) => entries.is_empty(),
                None => !dict.fsa.has_sequence(),
            },
            Source::Decoded { materialized, .. } => {
                materialized.entries.get().is_none_or(|e| e.is_empty())
            }
        }
    }
}

pub struct SynthDictionary {
    fsa: Fsa,
    separator: u8,
}

impl SynthDictionary {
    pub fn load(dict_path: &Path, info_path: &Path) -> Option<Self> {
        let data: std::sync::Arc<[u8]> =
            std::sync::Arc::from(std::fs::read(dict_path).ok()?.into_boxed_slice());
        let fsa = Fsa::from_bytes(data)?;
        let info = std::fs::read_to_string(info_path).ok()?;
        let separator = info
            .lines()
            .find_map(|l| l.trim().strip_prefix("fsa.dict.separator="))?
            .trim()
            .as_bytes()
            .first()
            .copied()?;
        Some(Self { fsa, separator })
    }

    pub fn lookup(&self, key: &str) -> Vec<String> {
        let Some(node) = self.fsa.seek(key.as_bytes()) else {
            return Vec::new();
        };
        let mut encoded: Vec<Vec<u8>> = Vec::new();
        self.fsa
            .separator_suffixes(node, self.separator, &mut |seq: &[u8]| {
                encoded.push(seq.to_vec())
            });
        encoded
            .iter()
            .filter_map(|bytes| {
                let field = match bytes.iter().position(|b| *b == self.separator) {
                    Some(pos) => &bytes[..pos],
                    None => bytes.as_slice(),
                };
                decode_suffix(key.as_bytes(), field).and_then(|v| String::from_utf8(v).ok())
            })
            .collect()
    }
}

pub fn shared_synth_dictionary(
    resource_dir: &Path,
    dict_stem: &str,
) -> Option<std::sync::Arc<SynthDictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<std::path::PathBuf, std::sync::Weak<SynthDictionary>>,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = resource_dir.join(format!("{dict_stem}.dict"));
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(&key).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let dictionary = std::sync::Arc::new(SynthDictionary::load(
        &key,
        &resource_dir.join(format!("{dict_stem}.info")),
    )?);
    guard.insert(key, std::sync::Arc::downgrade(&dictionary));
    Some(dictionary)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fsa5_fixture_bytes() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"\\fsa");
        data.extend_from_slice(&[0x05, b'_', b'+', 0x02]);
        let mut arcs = vec![0u8; 17];
        arcs[0] = 0x00;
        arcs[1] = 0x04;
        arcs[2] = 0x00;
        arcs[3] = 0x28;
        arcs[4] = 0x00;
        arcs[5] = 0x61;
        arcs[6] = 0x42;
        arcs[7] = 0x00;
        arcs[8] = 0x2b;
        arcs[9] = 0x70;
        arcs[10] = 0x00;
        arcs[11] = 0x62;
        arcs[12] = 0x8b;
        arcs[13] = 0x00;
        arcs[14] = 0x7a;
        arcs[15] = 0x8b;
        arcs[16] = 0x00;
        data.extend_from_slice(&arcs);
        data
    }

    fn fsa5_fixture() -> Fsa {
        Fsa::from_bytes(std::sync::Arc::from(
            fsa5_fixture_bytes().into_boxed_slice(),
        ))
        .expect("fsa5 fixture")
    }

    #[test]
    fn fsa5_reader_walks_seeks_and_answers_contains_word() {
        let fsa = fsa5_fixture();
        let Fsa::V5(fsa5) = &fsa else {
            panic!("fixture must dispatch to the FSA5 reader");
        };
        let root = fsa5.root_node().expect("root node");
        assert_eq!(root, 5);

        let mut sequences = Vec::new();
        fsa.for_each_sequence(&mut |s: &[u8]| sequences.push(s.to_vec()));
        assert_eq!(sequences, [b"a+z".to_vec(), b"ab".to_vec()]);

        assert!(fsa5.seek(root, b"ab").is_some());
        assert!(fsa5.seek(root, b"az").is_none());

        assert!(fsa5.contains_word(root, b"a", b'+'));
        assert!(fsa5.contains_word(root, b"ab", b'+'));
        assert!(fsa5.contains_word(root, b"a+z", b'+'));
        assert!(!fsa5.contains_word(root, b"a+", b'+'));
        assert!(!fsa5.contains_word(root, b"z", b'+'));
        assert!(!fsa5.contains_word(root, b"b", b'+'));
        assert!(!fsa5.contains_word(root, b"aq", b'+'));
    }

    pub(super) fn cfsa1_fixture_bytes() -> Vec<u8> {
        let mut data = vec![0u8; 55];
        data[0..4].copy_from_slice(b"\\fsa");
        data[4] = 0xc5;
        data[7] = 0x02;
        data[9] = b'z';
        let arcs_start = 40;
        let mut set = |offset: usize, bytes: &[u8]| {
            data[arcs_start + offset..arcs_start + offset + bytes.len()].copy_from_slice(bytes);
        };
        set(0, &[0x04, 0x00]);
        set(2, &[0x28, 0x00, 0x00]);
        set(5, &[0x42, 0x61, 0x00]);
        set(8, &[0x70, 0x2b, 0x00]);
        set(11, &[0x7b, 0x62, 0x00]);
        set(14, &[0x0f]);
        data
    }

    fn cfsa1_fixture() -> Fsa {
        Fsa::from_bytes(std::sync::Arc::from(
            cfsa1_fixture_bytes().into_boxed_slice(),
        ))
        .expect("cfsa1 fixture")
    }

    #[test]
    fn cfsa1_reader_decodes_labels_walks_and_seeks() {
        let fsa = cfsa1_fixture();
        let Fsa::C1(cfsa1) = &fsa else {
            panic!("fixture must dispatch to the CFSA1 reader");
        };
        let root = cfsa1.root_node().expect("root node");
        assert_eq!(root, 5);

        let mut sequences = Vec::new();
        fsa.for_each_sequence(&mut |s: &[u8]| sequences.push(s.to_vec()));
        assert_eq!(sequences, [b"a+z".to_vec(), b"ab".to_vec()]);

        assert!(cfsa1.seek(root, b"ab").is_some());
        assert!(cfsa1.seek(root, b"az").is_none());

        assert!(cfsa1.contains_word(root, b"a", b'+'));
        assert!(cfsa1.contains_word(root, b"ab", b'+'));
        assert!(cfsa1.contains_word(root, b"a+z", b'+'));
        assert!(!cfsa1.contains_word(root, b"a+", b'+'));
        assert!(!cfsa1.contains_word(root, b"z", b'+'));
    }

    #[test]
    fn fsa_rejects_unknown_versions() {
        let mut data = Vec::new();
        data.extend_from_slice(b"\\fsa");
        data.push(0x07);
        data.extend_from_slice(&[0, 0, 0x02]);
        data.extend_from_slice(&[0u8; 64]);
        assert!(Fsa::from_bytes(std::sync::Arc::from(data.into_boxed_slice())).is_none());
    }

    #[test]
    fn fsa5_and_cfsa1_reject_zero_goto_length_headers() {
        for version in [0x05u8, 0xc5] {
            let mut data = Vec::new();
            data.extend_from_slice(b"\\fsa");
            data.push(version);
            data.extend_from_slice(&[0, 0, 0x00]);
            data.extend_from_slice(&[0u8; 64]);
            assert!(Fsa::from_bytes(std::sync::Arc::from(data.into_boxed_slice())).is_none());
        }
    }

    #[test]
    fn arabic_synth_lookup_decodes_suffix_encoded_forms() {
        let dir = std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        );
        let Some(dict) = SynthDictionary::load(
            &dir.join("arabic_synth.dict"),
            &dir.join("arabic_synth.info"),
        ) else {
            return;
        };
        let forms = dict.lookup("آب|NJ-;F1--;---");
        assert_eq!(
            forms,
            vec!["آبة".to_string()],
            "synth lookup must decode the suffix-encoded form"
        );
        assert!(dict.lookup("آب|NONEXISTENT-TAG").is_empty());
    }

    #[test]
    fn dump_obdachloser_sequences() {
        let dir = dict_path().expect("dict");
        let data: std::sync::Arc<[u8]> = std::sync::Arc::from(
            std::fs::read(dir.join("german.dict"))
                .unwrap()
                .into_boxed_slice(),
        );
        let fsa = Fsa::from_bytes(data).expect("fsa");
        fsa.for_each_sequence(&mut |seq: &[u8]| {
            if seq.starts_with(b"Obdachlos") {
                eprintln!("RAW: {}", String::from_utf8_lossy(seq));
            }
        });
    }

    fn dict_path() -> Option<std::path::PathBuf> {
        [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
            "languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("german.dict").exists())
    }

    fn lang_dir(lang: &str, file: &str) -> Option<std::path::PathBuf> {
        [
            format!("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"),
            format!("languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"),
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join(file).exists())
    }

    fn cross_check_lazy_lookup_matches_enumeration(dir: &std::path::Path, stem: &str) {
        let dict = MorfologikDictionary::load(
            &dir.join(format!("{stem}.dict")),
            &dir.join(format!("{stem}.info")),
        )
        .unwrap_or_else(|| panic!("load {stem} dict"));
        let entries = dict.entries_map();
        assert!(!entries.is_empty(), "{stem} dictionary must enumerate");
        for (word, readings) in entries.iter() {
            let lazy = dict.lookup(word);
            let diverged = lazy.len() != readings.len()
                || lazy
                    .iter()
                    .zip(readings.iter())
                    .any(|(l, e)| l.0 != e.0.as_ref() || l.1 != e.1.as_ref());
            assert!(
                !diverged,
                "{stem}: lazy lookup diverged for {word}: {lazy:?} vs {readings:?}"
            );
        }
        assert!(dict.lookup("__no_such_word_zq__").is_empty());
    }

    static DICT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn suffix_decoder_matches_java_examples() {
        assert_eq!(
            decode_suffix(b"foo", b"Abar").as_deref(),
            Some(b"foobar".as_slice())
        );
        assert_eq!(
            decode_suffix(b"foo", b"Dbar").as_deref(),
            Some(b"bar".as_slice())
        );
    }

    #[test]
    fn prefix_and_suffix_decoder_matches_java_examples() {
        assert_eq!(
            decode_prefix_and_suffix(b"abc", b"AAd").as_deref(),
            Some(b"abcd".as_slice())
        );
        assert_eq!(
            decode_prefix_and_suffix(b"abc", b"ADxyz").as_deref(),
            Some(b"xyz".as_slice())
        );
        assert_eq!(
            decode_prefix_and_suffix("kwietnia".as_bytes(), b"AEcie\xc5\x84".as_ref()).as_deref(),
            Some("kwiecień".as_bytes())
        );
    }

    #[test]
    fn from_decoded_text_builds_lemma_inversion() {
        let data = "friend\tfriend\tNN\nfriends\tfriend\tNNS\nfriendly\tfriendly\tJJ\n";
        let dict = MorfologikDictionary::from_decoded_text(data);
        assert_eq!(dict.lookup("friends").len(), 1);
        let forms = dict.forms_by_lemma();
        let friend = forms.get("friend").expect("lemma inversion");
        assert_eq!(friend.len(), 2);
        let joined: Vec<&str> = friend.iter().map(|(t, _)| &**t).collect();
        assert!(joined.contains(&"NN") && joined.contains(&"NNS"));
        assert!(friend.iter().any(|(_, f)| &**f == "friends"));
    }

    #[test]
    fn final_arcs_keep_their_continuations() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = match dict_path() {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("german.dict"), &dir.join("german.info"))
            .expect("load german dict");
        let readings = dict.lookup("Abgeordnete");
        assert!(
            readings
                .iter()
                .any(|(_, tag)| *tag == "SUB:NOM:SIN:MAS:ADJ"),
            "Abgeordnete must keep the :ADJ reading behind the plain-SUB prefix: {readings:?}"
        );
        assert!(
            readings
                .iter()
                .any(|(_, tag)| *tag == "SUB:NOM:SIN:FEM:ADJ"),
            "Abgeordnete must keep SUB:NOM:SIN:FEM:ADJ: {readings:?}"
        );
        assert_eq!(
            readings.len(),
            10,
            "Abgeordnete has 10 dict entries: {readings:?}"
        );
    }

    #[test]
    fn decodes_suffix_encoded_lemmas() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = match dict_path() {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("german.dict"), &dir.join("german.info"));
        let dict = dict.expect("load german dict");
        assert!(dict.len() > 100_000, "got {}", dict.len());

        let haus = dict.lookup("Haus");
        assert!(
            haus.iter().any(|(lemma, tag)| lemma == &"Haus"
                && tag.starts_with("SUB:")
                && tag.contains(":SIN:")
                && tag.ends_with(":NEU")),
            "Haus readings: {haus:?}"
        );

        let form = "Abgeordnetenhäusern";
        let got = dict.lookup(form);
        assert!(
            got.iter()
                .any(|(lemma, tag)| lemma == &"Abgeordnetenhaus" && tag.contains("DAT:PLU")),
            "{form} readings: {got:?}"
        );
    }

    #[test]
    fn italian_fsa5_lookups_match_the_java_tagger_test() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/it/src/main/resources/org/languagetool/resource/it",
            "languagetool/languagetool-language-modules/it/src/main/resources/org/languagetool/resource/it",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("italian.dict").exists());
        let dir = match dir {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("italian.dict"), &dir.join("italian.info"));
        let dict = dict.expect("load italian dict (FSA5, iso-8859-15, suffix encoder)");
        assert!(dict.has_entries(), "italian dict must expose entries");

        let senza = dict.lookup("senza");
        assert!(
            senza.contains(&("senza", "CON")) && senza.contains(&("senza", "PRE")),
            "senza readings: {senza:?}"
        );

        let linguaggio = dict.lookup("linguaggio");
        assert!(
            linguaggio
                .iter()
                .any(|(lemma, tag)| lemma == &"linguaggio" && *tag == "NOUN-M:s"),
            "linguaggio readings: {linguaggio:?}"
        );

        let inganno = dict.lookup("inganno");
        assert!(
            inganno
                .iter()
                .any(|(lemma, tag)| lemma == &"ingannare" && *tag == "VER:ind+pres+1+s"),
            "inganno readings: {inganno:?}"
        );
        assert!(
            inganno
                .iter()
                .any(|(lemma, tag)| lemma == &"inganno" && *tag == "NOUN-M:s"),
            "inganno readings: {inganno:?}"
        );

        let impossibile = dict.lookup("impossibile");
        assert!(
            impossibile
                .iter()
                .any(|(lemma, tag)| lemma == &"impossibile"
                    && (*tag == "ADJ:pos+f+s" || *tag == "ADJ:pos+m+s")),
            "impossibile readings: {impossibile:?}"
        );

        let e_accented = dict.lookup("è");
        assert!(
            e_accented.iter().any(|(lemma, tag)| lemma == &"essere"
                && (*tag == "AUX:ind+pres+3+s" || *tag == "VER:ind+pres+3+s")),
            "è readings: {e_accented:?}"
        );
    }

    #[test]
    fn lazy_lookup_matches_eager_enumeration_german() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = dict_path() else {
            return;
        };
        cross_check_lazy_lookup_matches_enumeration(&dir, "german");
    }

    #[test]
    fn lazy_lookup_matches_eager_enumeration_french_fsa5() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = lang_dir("fr", "french.dict") else {
            return;
        };
        cross_check_lazy_lookup_matches_enumeration(&dir, "french");
    }

    #[test]
    fn decodes_prefix_and_suffix_encoded_lemmas() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl",
            "languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("polish.dict").exists());
        let dir = match dir {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("polish.dict"), &dir.join("polish.info"));
        let dict = dict.expect("load polish dict");

        let maja = dict.lookup("maja");
        assert!(
            maja.iter()
                .any(|(lemma, tag)| lemma == &"maj" && tag.starts_with("subst:")),
            "maja readings: {maja:?}"
        );

        let kwietnia = dict.lookup("kwietnia");
        assert!(
            kwietnia
                .iter()
                .any(|(lemma, tag)| lemma == &"kwiecień" && tag.starts_with("subst:")),
            "kwietnia readings: {kwietnia:?}"
        );

        let trwalo = dict.lookup("trwało");
        assert!(
            trwalo
                .iter()
                .any(|(lemma, tag)| lemma == &"trwać" && tag.starts_with("verb:")),
            "trwało readings: {trwalo:?}"
        );
    }

    #[test]
    fn decodes_koi8_r_suffix_encoded_lemmas() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
            "languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("russian.dict").exists());
        let dir = match dir {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("russian.dict"), &dir.join("russian.info"));
        let dict = dict.expect("load russian dict");

        let imelos = dict.lookup("имелось");
        assert!(
            imelos
                .iter()
                .any(|(lemma, tag)| lemma == &"иметься" && tag.starts_with("VB:Past")),
            "имелось readings: {imelos:?}"
        );

        let kauril = dict.lookup("караулила");
        assert!(
            kauril
                .iter()
                .any(|(lemma, tag)| lemma == &"караулить" && tag.starts_with("VB:Past")),
            "караулила readings: {kauril:?}"
        );
    }

    #[test]
    #[ignore]
    fn lazy_lookup_matches_eager_enumeration_russian_koi8r() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = lang_dir("ru", "russian.dict") else {
            return;
        };
        cross_check_lazy_lookup_matches_enumeration(&dir, "russian");
    }

    #[test]
    #[ignore]
    fn lazy_lookup_matches_eager_enumeration_italian_latin15() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = lang_dir("it", "italian.dict") else {
            return;
        };
        cross_check_lazy_lookup_matches_enumeration(&dir, "italian");
    }

    #[test]
    fn lazy_lookup_matches_eager_enumeration_asturian_cfsa1() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = lang_dir("ast", "asturian.dict") else {
            return;
        };
        cross_check_lazy_lookup_matches_enumeration(&dir, "asturian");
    }

    #[test]
    fn loads_cfsa1_version_cfsa_dict() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ast/src/main/resources/org/languagetool/resource/ast",
            "languagetool/languagetool-language-modules/ast/src/main/resources/org/languagetool/resource/ast",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("asturian.dict").exists());
        let dir = match dir {
            Some(d) => d,
            None => return,
        };
        let dict =
            MorfologikDictionary::load(&dir.join("asturian.dict"), &dir.join("asturian.info"));
        let dict = dict.expect("load asturian dict (CFSA version 0xc5)");

        assert!(!dict.is_empty(), "asturian dictionary must not be empty");

        let va = dict.lookup("va");
        assert!(
            va.iter()
                .any(|(lemma, tag)| lemma == &"dir" && tag.starts_with("VMIP")),
            "va readings: {va:?}"
        );

        let entamu = dict.lookup("entamu");
        assert!(
            entamu
                .iter()
                .any(|(lemma, tag)| lemma == &"entamu" && tag.starts_with("NCMS")),
            "entamu readings: {entamu:?}"
        );
    }

    #[test]
    fn slovak_v6_prefix_dict_lookups_match_slovak_tagger_test() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/sk/src/main/resources/org/languagetool/resource/sk",
            "languagetool/languagetool-language-modules/sk/src/main/resources/org/languagetool/resource/sk",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("slovak.dict").exists());
        let dir = match dir {
            Some(d) => d,
            None => return,
        };
        let dict = MorfologikDictionary::load(&dir.join("slovak.dict"), &dir.join("slovak.info"));
        let dict = dict.expect("load slovak dict (CFSA version 0xc6, prefix encoder)");

        let tu = dict.lookup("tu");
        let tu_tags: Vec<&str> = tu.iter().map(|(_, t)| *t).collect();
        for tag in ["J", "PD", "T"] {
            assert!(tu_tags.contains(&tag), "tu tags: {tu:?}");
        }

        let najdete = dict.lookup("nájdete");
        assert!(
            najdete
                .iter()
                .any(|(lemma, tag)| lemma == &"nájsť" && *tag == "VKdpb+"),
            "nájdete readings: {najdete:?}"
        );

        let cisla = dict.lookup("čísla");
        assert!(
            cisla.iter().any(|(lemma, tag)| lemma == &"číslo"
                && ["SSnp1", "SSnp4", "SSnp5", "SSns2"].contains(tag)),
            "čísla readings: {cisla:?}"
        );

        let kultura = dict.lookup("kultúra");
        assert!(
            kultura
                .iter()
                .any(|(lemma, tag)| lemma == &"kultúra" && ["SSfs1", "SSfs5"].contains(tag)),
            "Kultúra readings: {kultura:?}"
        );
    }
    #[test]
    fn is_empty_answers_before_materialization_and_agrees_after() {
        let _guard = DICT_TEST_LOCK.lock().unwrap();
        let Some(dir) = dict_path() else {
            return;
        };
        let dict = MorfologikDictionary::load(&dir.join("german.dict"), &dir.join("german.info"))
            .expect("load german dict");
        assert!(!dict.is_empty());
        let len = dict.len();
        assert!(len > 100_000, "german dict len: {len}");
        assert!(!dict.is_empty());

        let empty = MorfologikDictionary::from_decoded_text("");
        assert!(empty.is_empty());
        let one = MorfologikDictionary::from_decoded_text("haus\thaus\tNN");
        assert!(!one.is_empty());
    }
}

#[cfg(test)]
mod cfsa2_tests {
    use super::*;
    use std::hash::Hasher;

    fn vint(mut value: usize, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn arc(flags: u8, label: u8, dest: usize) -> Vec<u8> {
        let mut out = vec![flags, label];
        vint(dest, &mut out);
        out
    }

    const FINAL: u8 = 0x20;
    const LAST: u8 = 0x40;

    fn small_cfsa2() -> Vec<u8> {
        let mut a: Vec<u8> = vec![0x00, 0x00];
        vint(3, &mut a);
        a.extend(arc(FINAL, b'a', 9));
        a.extend(arc(LAST, b'l', 54));
        a.extend(arc(0x00, b'+', 15));
        a.extend(arc(FINAL | LAST, b'b', 24));
        a.extend(arc(LAST, b'x', 18));
        a.extend(arc(LAST, b'+', 21));
        a.extend(arc(FINAL | LAST, b'y', 0));
        a.extend(arc(LAST, b'+', 27));
        a.extend(arc(LAST, b'p', 30));
        a.extend(arc(LAST, b'+', 33));
        for (i, b) in b"VER:INF".iter().enumerate() {
            let last = i == b"VER:INF".len() - 1;
            let flags = if last { FINAL | LAST } else { LAST };
            let dest = if last { 0 } else { a.len() + 3 };
            a.extend(arc(flags, *b, dest));
        }
        a.extend(arc(LAST, b'|', 57));
        a.extend(arc(FINAL | LAST, b'm', 60));
        a.extend(arc(LAST, b'+', 63));
        a.extend(arc(FINAL | LAST, b'A', 0));
        assert_eq!(a.len(), 66);
        a
    }

    fn cfsa2_bytes(data: Vec<u8>) -> Fsa {
        let mut file = b"\\fsa".to_vec();
        file.push(0xc6);
        file.extend_from_slice(&[0x00, 0x00]);
        file.push(0x00);
        file.extend_from_slice(&data);
        Fsa::from_bytes(std::sync::Arc::from(file.into_boxed_slice())).expect("cfsa2 fixture")
    }

    fn deep_cfsa2() -> Fsa {
        let mut a: Vec<u8> = vec![0x00, 0x00];
        vint(3, &mut a);
        a.extend(arc(FINAL | LAST, b'a', 6));
        a.extend(arc(LAST, b'+', 9));
        while a.len() < 3 * 150 {
            let next = if a.len() + 3 < 128 {
                a.len() + 3
            } else {
                a.len() + 4
            };
            a.extend(arc(LAST, b'x', next));
        }
        let next = if a.len() + 3 < 128 {
            a.len() + 3
        } else {
            a.len() + 4
        };
        a.extend(arc(LAST, b'+', next));
        a.extend(arc(FINAL | LAST, b'y', 0));
        cfsa2_bytes(a)
    }

    #[test]
    fn cfsa2_reader_walks_seeks_and_answers_contains_word() {
        let fsa = cfsa2_bytes(small_cfsa2());
        let Fsa::C2(c2) = &fsa else {
            panic!("fixture must dispatch to the CFSA2 reader");
        };
        assert_eq!(c2.root(), 3);

        let mut sequences = Vec::new();
        fsa.for_each_sequence(&mut |s: &[u8]| sequences.push(s.to_vec()));
        assert_eq!(
            sequences,
            [
                b"a".to_vec(),
                b"a+x+y".to_vec(),
                b"ab".to_vec(),
                b"ab+p+VER:INF".to_vec(),
                b"l|m".to_vec(),
                b"l|m+A".to_vec(),
            ]
        );

        assert!(fsa.seek(b"a").is_some());
        assert!(fsa.seek(b"ab").is_some());
        assert!(fsa.seek(b"l|m").is_some());
        assert!(fsa.seek(b"zzz").is_none());

        assert!(fsa.contains_word(b"a", b'+'));
        assert!(fsa.contains_word(b"ab", b'+'));
        assert!(fsa.contains_word(b"l|m", b'+'));
        assert!(!fsa.contains_word(b"c", b'+'));
        assert!(fsa.has_sequence());
    }

    #[test]
    fn cfsa2_reader_handles_multibyte_vint_destinations() {
        let fsa = deep_cfsa2();
        let mut lengths = Vec::new();
        fsa.for_each_sequence(&mut |s: &[u8]| lengths.push(s.len()));
        assert_eq!(lengths.len(), 2);
        assert_eq!(lengths[0], 1);
        assert!(lengths[1] > 100, "deep sequence length: {lengths:?}");

        assert!(fsa.seek(b"a").is_some());
        assert!(fsa.contains_word(b"a", b'+'));
    }

    #[test]
    fn hasher_integer_writes_agree_with_repeated_runs() {
        let mut first = FxHasher::default();
        first.write_u8(0x7f);
        first.write_u16(0xbeef);
        first.write_u32(0xdead_beef);
        first.write_u64(0x0123_4567_89ab_cdef);
        first.write_usize(42);
        let mut second = FxHasher::default();
        second.write_u8(0x7f);
        second.write_u16(0xbeef);
        second.write_u32(0xdead_beef);
        second.write_u64(0x0123_4567_89ab_cdef);
        second.write_usize(42);
        assert_eq!(first.finish(), second.finish());

        let mut other = FxHasher::default();
        other.write_u8(0x7e);
        other.write_u16(0xbeef);
        other.write_u32(0xdead_beef);
        other.write_u64(0x0123_4567_89ab_cdef);
        other.write_usize(42);
        assert_ne!(first.finish(), other.finish());
    }

    #[test]
    fn trim_frequency_tag_variants() {
        assert_eq!(trim_frequency_tag(false, "NN:1"), "NN:1");
        assert_eq!(trim_frequency_tag(true, "NN:1"), "NN:");
        assert_eq!(trim_frequency_tag(true, "1"), "1");
        assert_eq!(trim_frequency_tag(true, ""), "");
    }

    #[test]
    fn decode_suffix_variants() {
        assert_eq!(decode_suffix(b"haus", b""), None);
        assert_eq!(decode_suffix(b"haus", b"\xffaus"), None);
        assert_eq!(decode_suffix(b"haus", b"@aus"), Some(b"aus".to_vec()));
        assert_eq!(decode_suffix(b"haus", b"Ae"), Some(b"hause".to_vec()));
        assert_eq!(decode_suffix(b"haus", b"E"), Some(b"".to_vec()));
    }

    #[test]
    fn decode_prefix_and_suffix_variants() {
        assert_eq!(decode_prefix_and_suffix(b"word", b""), None);
        assert_eq!(decode_prefix_and_suffix(b"word", b"A"), None);
        assert_eq!(decode_prefix_and_suffix(b"word", b"\xff\xffx"), None);
        assert_eq!(
            decode_prefix_and_suffix(b"word", b"@@x"),
            Some(b"x".to_vec())
        );
        assert_eq!(
            decode_prefix_and_suffix(b"word", b"ABxy"),
            Some(b"worxy".to_vec())
        );
        assert_eq!(
            decode_prefix_and_suffix(b"word", b"BCxy"),
            Some(b"oxy".to_vec())
        );
        assert_eq!(decode_prefix_and_suffix(b"word", b"DCxy"), None);
    }

    #[test]
    fn dict_encoding_parses_every_alias() {
        assert!(matches!(DictEncoding::parse("koi8-r"), DictEncoding::Koi8R));
        assert!(matches!(
            DictEncoding::parse(" KOI8-R "),
            DictEncoding::Koi8R
        ));
        assert!(matches!(
            DictEncoding::parse("windows-1251"),
            DictEncoding::Windows1251
        ));
        assert!(matches!(
            DictEncoding::parse("cp1251"),
            DictEncoding::Windows1251
        ));
        assert!(matches!(
            DictEncoding::parse("iso-8859-1"),
            DictEncoding::Latin1
        ));
        assert!(matches!(
            DictEncoding::parse("ISO8859-1"),
            DictEncoding::Latin1
        ));
        assert!(matches!(
            DictEncoding::parse("iso-8859-15"),
            DictEncoding::Latin15
        ));
        assert!(matches!(
            DictEncoding::parse("iso8859-15"),
            DictEncoding::Latin15
        ));
        assert!(matches!(
            DictEncoding::parse("anything-else"),
            DictEncoding::Utf8
        ));
    }

    #[test]
    fn dict_encoding_decodes_single_byte_charsets() {
        assert_eq!(DictEncoding::Koi8R.decode(&[0xe6]), "Ф");
        assert_eq!(DictEncoding::Koi8R.decode(&[0xd0]), "п");
        assert_eq!(DictEncoding::Windows1251.decode(&[0xff]), "я");
        assert_eq!(DictEncoding::Windows1251.decode(&[0xef]), "п");
        assert_eq!(DictEncoding::Latin1.decode(&[0x80]), "€");
        assert_eq!(DictEncoding::Latin15.decode(&[0xa4]), "€");
        assert_eq!(DictEncoding::Latin15.decode(&[0xa6]), "Š");
        assert_eq!(DictEncoding::Latin15.decode(&[0xa8]), "š");
        assert_eq!(DictEncoding::Latin15.decode(&[0xb4]), "Ž");
        assert_eq!(DictEncoding::Latin15.decode(&[0xb8]), "ž");
        assert_eq!(DictEncoding::Latin15.decode(&[0xbc]), "Œ");
        assert_eq!(DictEncoding::Latin15.decode(&[0xbd]), "œ");
        assert_eq!(DictEncoding::Latin15.decode(&[0xbe]), "Ÿ");
        assert_eq!(DictEncoding::Utf8.decode(&[0x61, 0xff]), "a\u{fffd}");
        assert_eq!(decode_bytes("koi8-r", &[0xc1]), "а");
    }

    #[test]
    fn dict_encoding_encodes_and_rejects_unmappable_text() {
        assert_eq!(
            DictEncoding::Utf8.encode("häus"),
            Some(b"h\xc3\xa4us".to_vec())
        );
        assert_eq!(
            DictEncoding::Koi8R.encode("дом"),
            Some(vec![0xc4, 0xcf, 0xcd])
        );
        assert_eq!(DictEncoding::Koi8R.encode("€"), None);
        assert_eq!(DictEncoding::Windows1251.encode("я"), Some(vec![0xff]));
        assert_eq!(DictEncoding::Latin1.encode("é"), Some(vec![0xe9]));
        assert_eq!(DictEncoding::Latin15.encode("€"), Some(vec![0xa4]));
        assert_eq!(DictEncoding::Latin15.encode("Š"), Some(vec![0xa6]));
        assert_eq!(DictEncoding::Latin15.encode("š"), Some(vec![0xa8]));
        assert_eq!(DictEncoding::Latin15.encode("Ž"), Some(vec![0xb4]));
        assert_eq!(DictEncoding::Latin15.encode("ž"), Some(vec![0xb8]));
        assert_eq!(DictEncoding::Latin15.encode("Œ"), Some(vec![0xbc]));
        assert_eq!(DictEncoding::Latin15.encode("œ"), Some(vec![0xbd]));
        assert_eq!(DictEncoding::Latin15.encode("Ÿ"), Some(vec![0xbe]));
    }

    #[test]
    fn info_value_finds_keys_and_trims() {
        let info = "fsa.dict.encoding = utf8\nother=x\nfsa.dict.separator=+\n";
        assert_eq!(
            info_value(info, "fsa.dict.separator="),
            Some("+".to_string())
        );
        assert_eq!(
            info_value(info, "fsa.dict.encoding ="),
            Some("utf8".to_string())
        );
        assert_eq!(info_value(info, "missing="), None);
    }

    fn with_dict_files(data: &[u8], info: &str, f: impl FnOnce(&Path, &Path)) {
        let unique = format!(
            "og_tagger_morf_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dict = dir.join("test.dict");
        let info_path = dir.join("test.info");
        std::fs::write(&dict, data).expect("write dict");
        std::fs::write(&info_path, info).expect("write info");
        f(&dict, &info_path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn cfsa2_file(data: Vec<u8>) -> Vec<u8> {
        let mut file = b"\\fsa".to_vec();
        file.push(0xc6);
        file.extend_from_slice(&[0x00, 0x00, 0x00]);
        file.extend_from_slice(&data);
        file
    }

    const INFO: &str = "fsa.dict.separator=+\nfsa.dict.encoding=utf8\n";

    #[test]
    fn fsa_word_set_loads_and_answers_membership() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, info| {
            let set = FsaWordSet::load(dict, info).expect("word set loads");
            assert!(!set.is_empty());
            assert!(set.contains("a"));
            assert!(set.contains("ab"));
            assert!(set.contains("l|m"));
            assert!(!set.contains(""));
            assert!(!set.contains("zzz"));
            assert_eq!(set.word_count(), 3);

            let mut words = Vec::new();
            set.for_each_word(|w| words.push(w.to_string()));
            assert_eq!(words.len(), 6);
            assert_eq!(set.word_list(), vec!["a", "ab", "l|m"]);
        });
    }

    #[test]
    fn fsa_word_set_rejects_words_containing_the_separator() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, _info| {
            let set = FsaWordSet::load(dict, _info).expect("word set loads");
            assert!(!set.contains("a+x"));
        });
    }

    #[test]
    fn fsa_word_set_answers_membership_for_fsa5_and_cfsa1_files() {
        for bytes in [
            super::tests::fsa5_fixture_bytes(),
            super::tests::cfsa1_fixture_bytes(),
        ] {
            with_dict_files(&bytes, INFO, |dict, info| {
                let set = FsaWordSet::load(dict, info).expect("word set loads");
                assert!(set.contains("a"));
                assert!(set.contains("ab"));
                assert!(!set.contains("a+x"));
                assert!(!set.contains("zzz"));
            });
        }
    }

    #[test]
    fn load_fsa_word_list_returns_distinct_decoded_words() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, info| {
            let mut words = load_fsa_word_list(dict, info).expect("word list loads");
            words.sort();
            assert_eq!(words, vec!["a", "ab", "l|m"]);
        });
    }

    #[test]
    fn load_fsa_word_list_fails_without_separator_info() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, "fsa.dict.encoding=utf8\n", |dict, info| {
            assert!(load_fsa_word_list(dict, info).is_none());
        });
    }

    #[test]
    fn morfologik_dictionary_loads_lookups_and_materializes() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, info| {
            let d = MorfologikDictionary::load(dict, info).expect("dict loads");
            assert!(d.has_entries());
            assert_eq!(d.lookup("a"), vec![("x", "y")]);
            assert_eq!(d.lookup("ab"), vec![("p", "VER:INF")]);
            assert_eq!(d.lookup("l|m"), Vec::<(&str, &str)>::new());
            assert_eq!(d.lookup("zzz"), Vec::<(&str, &str)>::new());
            assert_eq!(d.lookup("a"), vec![("x", "y")]);

            assert_eq!(d.len(), 2);
            let mut words = d.word_list();
            words.sort();
            assert_eq!(words, vec!["a", "ab"]);
            let mut keys: Vec<&str> = d.keys().collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["a", "ab"]);
            assert_eq!(
                d.fsa_word_list().expect("fsa words"),
                vec!["a", "ab", "l|m"]
            );

            let forms = d.forms_by_lemma();
            let x_forms = forms.get("x").expect("x forms");
            assert_eq!(x_forms.len(), 1);
            assert_eq!(&*x_forms[0].0, "y");
            assert_eq!(&*x_forms[0].1, "a");
            assert_eq!(
                d.verb_forms_by_lemma().get("p").map(|v| v.as_slice()),
                Some(&["ab".to_string()][..])
            );
            let synth = d.synth_forms_by_lemma().expect("synth");
            let l_synth = synth.get("l").expect("l synth");
            assert_eq!(l_synth.len(), 1);
            assert_eq!(&*l_synth[0].0, "m");
            assert_eq!(&*l_synth[0].1, "A");
        });
    }

    #[test]
    fn morfologik_dictionary_load_rejects_missing_files() {
        assert!(
            MorfologikDictionary::load(
                Path::new("/nonexistent/x.dict"),
                Path::new("/nonexistent/x.info")
            )
            .is_none()
        );
        assert!(
            FsaWordSet::load(
                Path::new("/nonexistent/x.dict"),
                Path::new("/nonexistent/x.info")
            )
            .is_none()
        );
        assert!(
            SynthDictionary::load(
                Path::new("/nonexistent/x.dict"),
                Path::new("/nonexistent/x.info")
            )
            .is_none()
        );
    }

    #[test]
    fn synth_dictionary_decodes_identity_lemmas() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, info| {
            let synth = SynthDictionary::load(dict, info).expect("synth loads");
            assert_eq!(synth.lookup("l|m"), vec!["l|m".to_string()]);
            assert_eq!(synth.lookup("a"), Vec::<String>::new());
            assert_eq!(synth.lookup("zzz"), Vec::<String>::new());
        });
    }

    #[test]
    fn shared_synth_dictionary_caches_by_path() {
        let bytes = cfsa2_file(small_cfsa2());
        with_dict_files(&bytes, INFO, |dict, _info| {
            let dir = dict.parent().unwrap();
            let first = shared_synth_dictionary(dir, "test").expect("shared synth");
            let second = shared_synth_dictionary(dir, "test").expect("shared synth again");
            assert!(std::sync::Arc::ptr_eq(&first, &second));
            drop(first);
            drop(second);
        });
        assert!(shared_synth_dictionary(Path::new("/nonexistent"), "none").is_none());
    }

    #[test]
    fn from_decoded_text_skips_malformed_lines_and_keeps_verbs() {
        let d = MorfologikDictionary::from_decoded_text(
            "# comment\n\nhaus\thaus\tNN\nbadline\nform\t\tVER:3:SIN\nform2\tlemma\n",
        );
        assert!(d.has_entries());
        assert_eq!(d.lookup("haus"), vec![("haus", "NN")]);
        assert_eq!(d.lookup("form"), vec![("form", "VER:3:SIN")]);
        assert_eq!(d.lookup("form2"), Vec::<(&str, &str)>::new());
        assert!(d.fsa_word_list().is_none());
        assert!(d.synth_forms_by_lemma().is_none());
        assert_eq!(
            d.verb_forms_by_lemma().get("form").map(|v| v.as_slice()),
            Some(&["form".to_string()][..])
        );
        let mut words = d.word_list();
        words.sort();
        assert_eq!(words, vec!["form", "haus"]);
    }
}
