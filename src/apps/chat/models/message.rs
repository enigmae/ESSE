use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tdn::types::{
    group::{EventId, GroupId},
    primitive::{PeerId, Result},
    rpc::{json, RpcParam},
};
use tdn_storage::local::{DStorage, DsValue};

use crate::storage::{
    read_avatar_sync, read_file_sync, read_image_sync, read_record_sync, write_avatar_sync,
    write_file_sync, write_image_sync, write_record_sync,
};

/// message type use in network.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) enum NetworkMessage {
    String(String),                            // content
    Image(Vec<u8>),                            // image bytes.
    File(String, Vec<u8>),                     // filename, file bytes.
    Contact(String, GroupId, PeerId, Vec<u8>), // name, gid, addr, avatar bytes.
    Record(Vec<u8>, u32),                      // record audio bytes.
    Emoji,
    Phone,
    Video,
    Invite(String),
    None,
}

impl NetworkMessage {
    pub(crate) fn handle(
        self,
        is_me: bool,
        gid: GroupId,
        base: &PathBuf,
        db: &DStorage,
        fid: i64,
        hash: EventId,
    ) -> Result<(Message, String)> {
        // handle event.
        let (m_type, raw) = match self {
            NetworkMessage::String(content) => (MessageType::String, content),
            NetworkMessage::Image(bytes) => {
                let image_name = write_image_sync(base, &gid, bytes)?;
                (MessageType::Image, image_name)
            }
            NetworkMessage::File(old_name, bytes) => {
                let filename = write_file_sync(base, &gid, &old_name, bytes)?;
                (MessageType::File, filename)
            }
            NetworkMessage::Contact(name, rgid, addr, avatar_bytes) => {
                write_avatar_sync(base, &gid, &rgid, avatar_bytes)?;
                let tmp_name = name.replace(";", "-;");
                let contact_values = format!("{};;{};;{}", tmp_name, rgid.to_hex(), addr.to_hex());
                (MessageType::Contact, contact_values)
            }
            NetworkMessage::Emoji => {
                // TODO
                (MessageType::Emoji, "".to_owned())
            }
            NetworkMessage::Record(bytes, time) => {
                let record_name = write_record_sync(base, &gid, fid, time, bytes)?;
                (MessageType::Record, record_name)
            }
            NetworkMessage::Phone => {
                // TODO
                (MessageType::Phone, "".to_owned())
            }
            NetworkMessage::Video => {
                // TODO
                (MessageType::Video, "".to_owned())
            }
            NetworkMessage::Invite(content) => (MessageType::Invite, content),
            NetworkMessage::None => {
                return Ok((
                    Message::new_with_id(
                        hash,
                        fid,
                        is_me,
                        MessageType::String,
                        "".to_owned(),
                        true,
                    ),
                    "".to_owned(),
                ));
            }
        };

        let scontent = match m_type {
            MessageType::String => {
                format!("{}:{}", m_type.to_int(), raw)
            }
            _ => format!("{}:", m_type.to_int()),
        };

        let mut msg = Message::new_with_id(hash, fid, is_me, m_type, raw, true);
        msg.insert(db)?;

        Ok((msg, scontent))
    }

    pub fn from_model(base: &PathBuf, gid: &GroupId, model: Message) -> Result<NetworkMessage> {
        // handle message's type.
        match model.m_type {
            MessageType::String => Ok(NetworkMessage::String(model.content)),
            MessageType::Image => {
                let bytes = read_image_sync(base, gid, &model.content)?;
                Ok(NetworkMessage::Image(bytes))
            }
            MessageType::File => {
                let bytes = read_file_sync(base, gid, &model.content)?;
                Ok(NetworkMessage::File(model.content, bytes))
            }
            MessageType::Contact => {
                let v: Vec<&str> = model.content.split(";;").collect();
                if v.len() != 3 {
                    return Ok(NetworkMessage::None);
                }
                let cname = v[0].to_owned();
                let cgid = GroupId::from_hex(v[1])?;
                let caddr = PeerId::from_hex(v[2])?;
                let avatar_bytes = read_avatar_sync(base, gid, &cgid)?;
                Ok(NetworkMessage::Contact(cname, cgid, caddr, avatar_bytes))
            }
            MessageType::Record => {
                let (bytes, time) = if let Some(i) = model.content.find('-') {
                    let time = model.content[0..i].parse().unwrap_or(0);
                    let bytes = read_record_sync(base, gid, &model.content[i + 1..])?;
                    (bytes, time)
                } else {
                    (vec![], 0)
                };
                Ok(NetworkMessage::Record(bytes, time))
            }
            MessageType::Invite => Ok(NetworkMessage::Invite(model.content)),
            MessageType::Emoji => Ok(NetworkMessage::Emoji),
            MessageType::Phone => Ok(NetworkMessage::Phone),
            MessageType::Video => Ok(NetworkMessage::Video),
        }
    }
}

#[derive(Eq, PartialEq)]
pub(crate) enum MessageType {
    String,
    Image,
    File,
    Contact,
    Emoji,
    Record,
    Phone,
    Video,
    Invite,
}

impl MessageType {
    pub fn to_int(&self) -> i64 {
        match self {
            MessageType::String => 0,
            MessageType::Image => 1,
            MessageType::File => 2,
            MessageType::Contact => 3,
            MessageType::Emoji => 4,
            MessageType::Record => 5,
            MessageType::Phone => 6,
            MessageType::Video => 7,
            MessageType::Invite => 8,
        }
    }

    pub fn from_int(i: i64) -> MessageType {
        match i {
            0 => MessageType::String,
            1 => MessageType::Image,
            2 => MessageType::File,
            3 => MessageType::Contact,
            4 => MessageType::Emoji,
            5 => MessageType::Record,
            6 => MessageType::Phone,
            7 => MessageType::Video,
            8 => MessageType::Invite,
            _ => MessageType::String,
        }
    }
}

pub(crate) struct Message {
    pub id: i64,
    pub hash: EventId,
    pub fid: i64,
    pub is_me: bool,
    pub m_type: MessageType,
    pub content: String,
    pub is_delivery: bool,
    pub datetime: i64,
    pub is_deleted: bool,
}

impl Message {
    pub fn new(
        gid: &GroupId,
        fid: i64,
        is_me: bool,
        m_type: MessageType,
        content: String,
        is_delivery: bool,
    ) -> Message {
        let start = SystemTime::now();
        let datetime = start
            .duration_since(UNIX_EPOCH)
            .map(|s| s.as_secs())
            .unwrap_or(0) as i64; // safe for all life.

        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&gid.0[0..8]);
        bytes[8..16].copy_from_slice(&(fid as u64).to_le_bytes()); // 8-bytes.
        bytes[16..24].copy_from_slice(&(datetime as u64).to_le_bytes()); // 8-bytes.
        let content_bytes = content.as_bytes();
        if content_bytes.len() >= 8 {
            bytes[24..32].copy_from_slice(&content_bytes[0..8]);
        } else {
            bytes[24..(24 + content_bytes.len())].copy_from_slice(&content_bytes);
        }

        Message {
            id: 0,
            hash: EventId(bytes),
            is_deleted: false,
            fid,
            is_me,
            m_type,
            content,
            is_delivery,
            datetime,
        }
    }

    pub fn new_with_id(
        hash: EventId,
        fid: i64,
        is_me: bool,
        m_type: MessageType,
        content: String,
        is_delivery: bool,
    ) -> Message {
        let start = SystemTime::now();
        let datetime = start
            .duration_since(UNIX_EPOCH)
            .map(|s| s.as_secs())
            .unwrap_or(0) as i64; // safe for all life.

        Message {
            id: 0,
            is_deleted: false,
            hash,
            fid,
            is_me,
            m_type,
            content,
            is_delivery,
            datetime,
        }
    }

    /// here is zero-copy and unwrap is safe. checked.
    fn from_values(mut v: Vec<DsValue>, contains_deleted: bool) -> Message {
        let is_deleted = if contains_deleted {
            v.pop().unwrap().as_bool()
        } else {
            false
        };

        Message {
            is_deleted,
            datetime: v.pop().unwrap().as_i64(),
            is_delivery: v.pop().unwrap().as_bool(),
            content: v.pop().unwrap().as_string(),
            m_type: MessageType::from_int(v.pop().unwrap().as_i64()),
            is_me: v.pop().unwrap().as_bool(),
            fid: v.pop().unwrap().as_i64(),
            hash: EventId::from_hex(v.pop().unwrap().as_str()).unwrap_or(EventId::default()),
            id: v.pop().unwrap().as_i64(),
        }
    }

    pub fn to_rpc(&self) -> RpcParam {
        json!([
            self.id,
            self.hash.to_hex(),
            self.fid,
            self.is_me,
            self.m_type.to_int(),
            self.content,
            self.is_delivery,
            self.datetime,
        ])
    }

    pub fn get(db: &DStorage, fid: &i64) -> Result<Vec<Message>> {
        let sql = format!("SELECT id, hash, fid, is_me, m_type, content, is_delivery, datetime FROM messages WHERE fid = {} and is_deleted = false ORDER BY id DESC", fid);
        let matrix = db.query(&sql)?;
        let mut messages = vec![];
        for values in matrix {
            messages.push(Message::from_values(values, false));
        }
        Ok(messages)
    }

    pub fn get_id(db: &DStorage, id: i64) -> Result<Option<Message>> {
        let sql = format!("SELECT id, hash, fid, is_me, m_type, content, is_delivery, datetime, is_deleted FROM messages WHERE id = {}", id);
        let mut matrix = db.query(&sql)?;
        if matrix.len() > 0 {
            let values = matrix.pop().unwrap(); // safe unwrap()
            return Ok(Some(Message::from_values(values, true)));
        }
        Ok(None)
    }

    pub fn get_it(db: &DStorage, hash: &EventId) -> Result<Option<Message>> {
        let sql = format!("SELECT id, hash, fid, is_me, m_type, content, is_delivery, datetime, is_deleted FROM messages WHERE hash = {}", hash.to_hex());
        let mut matrix = db.query(&sql)?;
        if matrix.len() > 0 {
            let values = matrix.pop().unwrap(); // safe unwrap()
            return Ok(Some(Message::from_values(values, true)));
        }
        Ok(None)
    }

    pub fn insert(&mut self, db: &DStorage) -> Result<()> {
        let sql = format!(
            "INSERT INTO messages (hash, fid, is_me, m_type, content, is_delivery, datetime, is_deleted) VALUES ('{}',{},{},{},'{}',{},{},false)",
            self.hash.to_hex(),
            self.fid,
            self.is_me,
            self.m_type.to_int(),
            self.content,
            self.is_delivery,
            self.datetime,
        );
        self.id = db.insert(&sql)?;
        Ok(())
    }

    pub fn delivery(db: &DStorage, id: i64, is_delivery: bool) -> Result<usize> {
        let sql = format!(
            "UPDATE messages SET is_delivery={} WHERE id = {}",
            is_delivery, id,
        );
        db.update(&sql)
    }

    pub fn delete(&self, db: &DStorage) -> Result<usize> {
        let sql = format!(
            "UPDATE messages SET is_deleted = true WHERE id = {}",
            self.id
        );
        db.delete(&sql)
    }

    pub fn exist(db: &DStorage, hash: &EventId) -> Result<bool> {
        let sql = format!("SELECT id FROM messages WHERE hash = '{}'", hash.to_hex());
        let matrix = db.query(&sql)?;
        Ok(matrix.len() > 0)
    }
}
