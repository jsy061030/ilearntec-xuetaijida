//! 数据模型。字段以抓包实测为准，尽量容忍学校接口的类型波动。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub realname: String,
    #[serde(default)]
    pub school_id: Option<String>,
    #[serde(default)]
    pub school_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub num: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub selected: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub course_id: Option<String>,
    #[serde(default)]
    pub course_name: Option<String>,
    #[serde(default)]
    pub teacher_id: Option<String>,
    #[serde(default)]
    pub teacher_name: Option<String>,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub term_id: Option<String>,
    #[serde(default)]
    pub term_year: Option<String>,
    #[serde(default)]
    pub term: Option<String>,
    #[serde(default)]
    pub status_name: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoClass {
    #[serde(default, deserialize_with = "crate::util::de_string_or_default")]
    pub video_class_id: String,
    #[serde(default, deserialize_with = "crate::util::de_string_or_default")]
    pub video_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    #[serde(default, deserialize_with = "crate::util::de_string_or_default")]
    pub id: String,
    #[serde(default, deserialize_with = "crate::util::de_string_or_default")]
    pub resource_id: String,
    #[serde(default)]
    pub live_record_name: Option<String>,
    #[serde(default)]
    pub course_name: Option<String>,
    #[serde(default)]
    pub teacher_name: Option<String>,
    #[serde(default)]
    pub building_name: Option<String>,
    #[serde(default)]
    pub room_name: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub current_week: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub current_day: Option<String>,
    #[serde(default)]
    pub current_date: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub section: Option<String>,
    #[serde(default)]
    pub time_range: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub live_status: Option<String>,
    #[serde(default)]
    pub video_path: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_vec_or_default")]
    pub video_class_map: Vec<VideoClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub video_code: Option<String>,
    #[serde(default)]
    pub video_name: Option<String>,
    #[serde(default)]
    pub video_path: Option<String>,
    #[serde(default)]
    pub video_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoResource {
    #[serde(default)]
    pub resource_name: Option<String>,
    #[serde(default)]
    pub phase_url: Option<String>,
    #[serde(default, deserialize_with = "crate::util::de_vec_or_default")]
    pub video_list: Vec<Video>,
    #[serde(default)]
    pub teacher_name: Option<String>,
    #[serde(default)]
    pub room_name: Option<String>,
    #[serde(default)]
    pub building_name: Option<String>,
    #[serde(default)]
    pub class_names: Option<String>,
    #[serde(default)]
    pub class_type: Option<i64>,
    #[serde(default)]
    pub resource_type: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekList {
    #[serde(default, deserialize_with = "crate::util::de_vec_or_default")]
    pub week_list: Vec<i64>,
    #[serde(default, deserialize_with = "crate::util::de_opt_string")]
    pub current_week: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Dump {
    pub generated_at_ms: i64,
    pub user: User,
    pub term: Term,
    pub weeks: Option<WeekList>,
    pub courses: Vec<CourseDump>,
}

#[derive(Debug, Serialize)]
pub struct CourseDump {
    pub lesson: Lesson,
    pub records: Vec<RecordDump>,
}

#[derive(Debug, Serialize)]
pub struct RecordDump {
    pub record: Record,
    pub video: Option<VideoResource>,
    pub video_error: Option<String>,
    pub subtitle_url: Option<String>,
    pub subtitle_file: Option<String>,
    pub subtitle_error: Option<String>,
}

/// 本地会话缓存：按域名保存 Cookie 的 `name=value` 列表。
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionFile {
    pub saved_at_ms: i64,
    pub cookies: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_user_mid_shape() {
        let v = json!({
            "schoolId": "x",
            "roles": [],
            "id": "u1",
            "pic": "",
            "units": [],
            "schoolName": "吉林大学",
            "splitKey": "s",
            "classList": [],
            "username": "jlu123",
            "realname": "张三"
        });
        let u: User = serde_json::from_value(v).unwrap();
        assert_eq!(u.username, "jlu123");
        assert_eq!(u.realname, "张三");
    }

    #[test]
    fn parse_term_shape() {
        let v = json!({
            "year": "2025-2026",
            "endDate": "2026-07-01",
            "num": "2",
            "name": "2025-2026学年第二学期",
            "id": "t1",
            "startDate": "2026-02-20",
            "selected": false
        });
        let t: Term = serde_json::from_value(v).unwrap();
        assert_eq!(t.year, "2025-2026");
        assert_eq!(t.num, "2");
        assert_eq!(t.id, "t1");
    }

    #[test]
    fn parse_record_with_video_class_map() {
        let v = json!({
            "id": "r1",
            "resourceId": "res1",
            "liveRecordName": "第12周",
            "buildingName": "逸夫楼",
            "currentWeek": 12,
            "currentDay": "5",
            "currentDate": "2026-05-15",
            "roomName": "A203",
            "section": "3,4",
            "timeRange": "10:00-11:40",
            "liveStatus": 3,
            "videoPath": "",
            "videoClassMap": [
                { "videoClassId": "v1", "videoName": "教师机位" },
                { "videoClassId": "v2", "videoName": "HDMI" }
            ]
        });
        let r: Record = serde_json::from_value(v).unwrap();
        assert_eq!(r.resource_id, "res1");
        assert_eq!(r.current_week.as_deref(), Some("12"));
        assert_eq!(r.video_class_map.len(), 2);
        assert_eq!(r.video_class_map[1].video_name, "HDMI");
    }

    #[test]
    fn parse_video_resource_shape() {
        let v = json!({
            "resourceName": "课程录播",
            "phaseUrl": "https://example.com/a.vtt",
            "videoList": [
                {
                    "id": "v1",
                    "videoCode": "1",
                    "videoName": "教师机位",
                    "videoPath": "https://example.com/a.mp4",
                    "videoSize": "123"
                }
            ]
        });
        let r: VideoResource = serde_json::from_value(v).unwrap();
        assert_eq!(r.resource_name.as_deref(), Some("课程录播"));
        assert_eq!(r.video_list.len(), 1);
        assert_eq!(r.video_list[0].video_size.as_deref(), Some("123"));
    }
}
