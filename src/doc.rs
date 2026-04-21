use automerge::{AutoCommit, ObjType, ReadDoc, ScalarValue, Value, transaction::Transactable};

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Pending,
    Active,
    Done,
    Abort,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Active => "ACTIVE",
            Self::Done => "DONE",
            Self::Abort => "ABORT",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "ACTIVE" => Self::Active,
            "DONE" => Self::Done,
            "ABORT" => Self::Abort,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Objective {
    pub task: String,
    pub status: Status,
    pub assignee: String,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub author: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Board {
    pub objectives: Vec<Objective>,
    pub notes: Vec<Note>,
}

pub struct Doc {
    inner: AutoCommit,
}

impl Doc {
    pub fn new() -> Self {
        let mut inner = AutoCommit::new();
        inner
            .put_object(automerge::ROOT, "objectives", ObjType::List)
            .unwrap();
        inner
            .put_object(automerge::ROOT, "notes", ObjType::List)
            .unwrap();
        Self { inner }
    }

    pub fn add_objective(&mut self, task: &str, assignee: &str) -> Vec<u8> {
        let obj_id = self.list_id("objectives");
        let len = self.inner.length(&obj_id);
        let item = self
            .inner
            .insert_object(&obj_id, len, ObjType::Map)
            .unwrap();
        self.inner.put(&item, "task", task).unwrap();
        self.inner.put(&item, "status", "PENDING").unwrap();
        self.inner.put(&item, "assignee", assignee).unwrap();
        self.inner.save()
    }

    pub fn set_status(&mut self, index: usize, status: &str) -> Vec<u8> {
        let obj_id = self.list_id("objectives");
        if let Ok(Some((_, item_id))) = self.inner.get(&obj_id, index) {
            self.inner.put(&item_id, "status", status).unwrap();
        }
        self.inner.save()
    }

    pub fn take_objective(&mut self, index: usize, operator: &str) -> Vec<u8> {
        let obj_id = self.list_id("objectives");
        if let Ok(Some((_, item_id))) = self.inner.get(&obj_id, index) {
            self.inner.put(&item_id, "assignee", operator).unwrap();
        }
        self.inner.save()
    }

    pub fn delete_objective(&mut self, index: usize) -> Vec<u8> {
        let obj_id = self.list_id("objectives");
        self.inner.delete(&obj_id, index).unwrap();
        self.inner.save()
    }

    pub fn add_note(&mut self, author: &str, text: &str) -> Vec<u8> {
        let obj_id = self.list_id("notes");
        let len = self.inner.length(&obj_id);
        let item = self
            .inner
            .insert_object(&obj_id, len, ObjType::Map)
            .unwrap();
        self.inner.put(&item, "author", author).unwrap();
        self.inner.put(&item, "text", text).unwrap();
        self.inner.save()
    }

    pub fn save(&mut self) -> Vec<u8> {
        self.inner.save()
    }

    pub fn merge_bytes(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let mut other = AutoCommit::load(bytes)?;
        self.inner.merge(&mut other)?;
        Ok(())
    }

    pub fn read(&self) -> Board {
        Board {
            objectives: self.read_objectives(),
            notes: self.read_notes(),
        }
    }

    fn list_id(&self, name: &str) -> automerge::ObjId {
        self.inner
            .get(automerge::ROOT, name)
            .unwrap()
            .map(|(_, id)| id)
            .unwrap()
    }

    fn read_objectives(&self) -> Vec<Objective> {
        let obj_id = match self.inner.get(automerge::ROOT, "objectives").unwrap() {
            Some((_, id)) => id,
            None => return vec![],
        };
        (0..self.inner.length(&obj_id))
            .filter_map(|i| {
                let (_, item_id) = self.inner.get(&obj_id, i).ok()??;
                let task = self.str_field(&item_id, "task")?;
                let status =
                    Status::from_str(&self.str_field(&item_id, "status").unwrap_or_default());
                let assignee = self.str_field(&item_id, "assignee").unwrap_or_default();
                Some(Objective {
                    task,
                    status,
                    assignee,
                })
            })
            .collect()
    }

    fn read_notes(&self) -> Vec<Note> {
        let obj_id = match self.inner.get(automerge::ROOT, "notes").unwrap() {
            Some((_, id)) => id,
            None => return vec![],
        };
        (0..self.inner.length(&obj_id))
            .filter_map(|i| {
                let (_, item_id) = self.inner.get(&obj_id, i).ok()??;
                let author = self.str_field(&item_id, "author")?;
                let text = self.str_field(&item_id, "text")?;
                Some(Note { author, text })
            })
            .collect()
    }

    fn str_field(&self, obj: &automerge::ObjId, key: &str) -> Option<String> {
        match self.inner.get(obj, key).ok()?? {
            (Value::Scalar(s), _) => match s.as_ref() {
                ScalarValue::Str(text) => Some(text.to_string()),
                _ => None,
            },
            _ => None,
        }
    }
}
