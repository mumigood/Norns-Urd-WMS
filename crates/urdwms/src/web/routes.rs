pub use super::dto::proposal_json;
use super::dto::{item_json, lot_json};
use crate::{
    auth::{self, Session},
    service::{self, Store},
    web::ApiError,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

type Result<T> = std::result::Result<T, ApiError>;
fn require(user: &Session, permission: &str) -> Result<()> {
    if user.can(permission) {
        Ok(())
    } else {
        Err(ApiError::new(
            403,
            format!("你的身分（{}）不能做這件事。需要權限：{}", user.role_label, service::permission_label(permission)),
        ))
    }
}
fn parse<T: DeserializeOwned>(body: Value) -> Result<T> {
    Ok(serde_json::from_value(body)?)
}
fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body.get(key).and_then(Value::as_str).ok_or_else(|| ApiError::new(400, format!("缺少欄位：{key}")))
}
fn number(body: &Value, key: &str) -> Result<i64> {
    body.get(key).and_then(Value::as_i64).ok_or_else(|| ApiError::new(400, format!("缺少整數欄位：{key}")))
}
fn normalize(mut body: Value, text_fields: &[&str], number_fields: &[&str]) -> Result<Value> {
    let map = body.as_object_mut().ok_or_else(|| ApiError::new(400, "請傳入 JSON 物件"))?;
    for key in text_fields {
        if map.get(*key) == Some(&Value::Null) {
            map.insert((*key).into(), json!(""));
        }
    }
    for key in number_fields {
        if map.get(*key) == Some(&Value::Null) {
            map.insert((*key).into(), json!(0));
        }
    }
    for key in ["has_expiry", "use_recognition", "active", "detail_pending"] {
        if let Some(n) = map.get(key).and_then(Value::as_i64) {
            if n != 0 && n != 1 {
                return Err(ApiError::new(400, "布林值必須是 0 或 1"));
            }
            map.insert(key.into(), json!(n == 1));
        }
    }
    Ok(body)
}
fn limit(query: &HashMap<String, String>, default: i64) -> Result<i64> {
    query
        .get("limit")
        .map(|v| v.parse::<i64>().map(|n| n.clamp(1, 10000)).map_err(|_| ApiError::new(400, "limit 必須是整數")))
        .unwrap_or(Ok(default))
}
fn query_id(query: &HashMap<String, String>, key: &str) -> Result<Option<i64>> {
    query.get(key).map(|v| v.parse().map_err(|_| ApiError::new(400, "識別碼必須是整數"))).transpose()
}

pub fn dispatch(
    store: &Store,
    user: &Session,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    body: Value,
) -> Result<Value> {
    let conn = &store.conn;
    match (method,path) {
        ("GET","/api/auth/signers")=>{
            let mut statement=conn.prepare("SELECT name,title,role FROM app_user WHERE active=1 ORDER BY role,name")?;
            let rows=statement.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
            Ok(json!(rows.iter().map(|(name,title,role)|json!({"name":name,"title":title,"role_label":auth::role_label(conn,role)})).collect::<Vec<_>>()))
        },
        ("GET","/api/items")=>Ok(json!(service::items(conn,if query.get("order").is_some_and(|v|v=="recent"){service::ItemOrder::Recent}else{service::ItemOrder::Sort})?.iter().map(item_json).collect::<Vec<_>>())),
        ("POST","/api/items")=>{require(user,"item.manage")?;let input=parse(normalize(body,&[],&[])?)?;Ok(json!({"id":service::create_item(store,user,&input)?}))},
        ("POST","/api/items/reorder")=>{require(user,"item.manage")?;let ids:Vec<i64>=parse(body.get("item_ids").cloned().unwrap_or(Value::Null))?;service::reorder_items(store,user,&ids)?;Ok(json!({"ok":true,"count":ids.len()}))},
        ("GET","/api/item-options")=>Ok(serde_json::to_value(service::item_options(conn)?)?),
        ("GET","/api/lots")=>Ok(json!(service::lots(conn,query_id(query,"item_id")?)?.iter().map(lot_json).collect::<Vec<_>>())),
        ("POST","/api/lots")=>{
            require(user,"lot.create")?;let mut body=normalize(body,&[],&[])?;
            let map=body.as_object_mut().unwrap();
            if let Some(v)=map.remove("qty_meters"){map.insert("qty_entered".into(),v);}
            map.entry("qty").or_insert(json!(1));map.entry("entered_unit").or_insert(json!("米"));
            let input:service::LotInput=parse(body)?;
            Ok(serde_json::to_value(service::create_lot(store,user,&input)?)?)
        },
        ("GET","/api/scans")=>scan_list(store,query),
        ("POST","/api/scans")=>{
            require(user,"issue.create")?;let mut body=normalize(body,&[],&[])?;body.as_object_mut().unwrap().entry("qty").or_insert(json!(1));
            let input:service::ScanInput=parse(body)?;
            if input.item_id<=0||input.qty<1 {return Err(ApiError::new(400,"品項與數量必須大於 0"));}
            Ok(serde_json::to_value(service::create_scan(store,user,&input)?)?)
        },
        ("GET","/api/audit")=>{require(user,"audit.read")?;Ok(serde_json::to_value(service::audit(store,user,query.get("action").map(String::as_str),limit(query,200)?)?)?)},
        ("GET","/api/alerts")=>alerts(store),
        ("GET","/api/dictionary")=>{
            let mut entries=Map::new();let mut categories=Map::new();
            for (code,label) in service::CATEGORIES {
                categories.insert((*code).into(),json!(label));
                let rows=if query.get("category").is_none_or(|v|v==code){service::dictionary(conn,code,query.get("include_inactive").is_some_and(|v|v=="true"||v=="1"))?}else{vec![]};
                entries.insert((*code).into(),json!(rows.iter().map(|r|json!({"id":r.id,"category":r.category,"value":r.value,"sort_order":r.sort_order,"active":i32::from(r.active)})).collect::<Vec<_>>()));
            }
            Ok(json!({"categories":categories,"entries":entries}))
        },
        ("POST","/api/dictionary")=>{
            require(user,"dictionary.manage")?;let category=text(&body,"category")?;let value=text(&body,"value")?.trim();
            let revived=service::add_entry(store,user,category,value)?;
            let id=service::dictionary(conn,category,true)?.into_iter().find(|e|e.value==value).map(|e|e.id);
            Ok(json!({"id":id,"value":value,"revived":revived}))
        },
        ("GET","/api/users")=>{
            require(user,"user.manage")?;
            let rows=service::users(store,user)?.into_iter().map(|u|{
                let mut v=serde_json::to_value(&u).unwrap();v["active"]=json!(i32::from(u.active));v["permissions"]=json!(auth::permissions(&u.role));
                let created:String=conn.query_row("SELECT created_at FROM app_user WHERE id=?",[u.id],|r|r.get(0)).unwrap_or_default();v["created_at"]=json!(created);v
            }).collect::<Vec<_>>();Ok(json!(rows))
        },
        ("POST","/api/users")=>{require(user,"user.manage")?;Ok(json!({"id":service::create_user(store,user,&parse(body)?)?}))},
        ("GET","/api/roles")=>Ok(json!(service::roles(conn).iter().map(|(code,label)|json!({"code":code,"label":label,"default_label":auth::default_role_label(code),"permissions":auth::permissions(code)})).collect::<Vec<_>>())),
        ("GET","/api/settings/recognition")=>Ok(json!({"model":service::recognition_model(conn),"models":service::RECOGNITION_MODELS.iter().map(|(value,label,note)|json!({"value":value,"label":label,"note":note})).collect::<Vec<_>>()})),
        ("PUT","/api/settings/recognition")=>{require(user,"dictionary.manage")?;let model=text(&body,"model")?;service::save_recognition_model(store,user,model)?;Ok(json!({"ok":true,"model":model}))},
        ("GET","/api/settings/alerts")=>Ok(serde_json::to_value(service::alert_thresholds(conn)?)?),
        ("PUT","/api/settings/alerts")=>{require(user,"dictionary.manage")?;let t=parse(body)?;service::save_alert_thresholds(store,user,t)?;Ok(serde_json::to_value(t)?)},
        ("GET","/api/settings/api-key")=>{require(user,"dictionary.manage")?;Ok(json!({"configured":service::api_key(conn).is_some(),"from_env":service::api_key_from_env()}))},
        ("PUT","/api/settings/api-key")=>{require(user,"dictionary.manage")?;service::save_api_key(store,user,text(&body,"key")?)?;Ok(json!({"ok":true}))},
        ("POST","/api/settings/api-key/test")=>{
            require(user,"dictionary.manage")?;
            let key=body.get("key").and_then(Value::as_str).filter(|k|!k.trim().is_empty()).map(str::to_string).or_else(||service::api_key(conn));
            let Some(key)=key else{return Ok(json!({"ok":false,"error":"尚未設定辨識金鑰"}));};
            let model=service::recognition_model(conn);let provider=urdwms_core::recognition::GeminiProvider::new(key.clone(),model.clone());
            Ok(match provider.check(){Ok(_)=>json!({"ok":true,"model":model}),Err(e)=>json!({"ok":false,"error":e.replace(&key,"[redacted]")})})
        },
        ("GET","/api/camera")=>{
            let config=service::camera_config(conn)?;let mut value=serde_json::to_value(&config)?;
            value.as_object_mut().unwrap().remove("password");value["has_password"]=json!(!config.password.is_empty());value["endpoint"]=json!(if config.configured(){Some(config.endpoint())}else{None});Ok(value)
        },
        ("PUT","/api/camera")=>{require(user,"dictionary.manage")?;service::save_camera(store,user,parse(body)?)?;Ok(json!({"ok":true,"endpoint":service::camera_config(conn)?.endpoint()}))},
        ("POST","/api/camera/test")=>{require(user,"dictionary.manage")?;camera_test(store)},
        ("GET","/api/camera/latest")=>{require(user,"issue.create")?;camera_latest(store)},
        ("POST","/api/camera/capture")=>{require(user,"issue.create")?;camera_capture(store)},
        ("POST","/api/resolve-item")=>{
            require(user,"issue.create")?;let id=number(&body,"item_id")?;
            if !service::items(conn,service::ItemOrder::Sort)?.iter().any(|i|i.id==id){return Err(ApiError::new(404,"品項不存在"));}
            let mut p=service::Proposal::default();p.recognition.receipt_date=body.get("ocr_receipt_date").and_then(Value::as_str).map(str::to_string);
            p.recognition.manufacture_date=body.get("ocr_manufacture_date").and_then(Value::as_str).map(str::to_string);
            service::resolve_item(conn,&mut p,id)?;proposal_json(store,&p)
        },
        _=>dynamic(store,user,method,path,body)
    }
}

fn dynamic(store: &Store, user: &Session, method: &str, path: &str, body: Value) -> Result<Value> {
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    if let ["api", "roles", code] = parts.as_slice()
        && method == "PATCH"
    {
        require(user, "user.manage")?;
        let label = text(&body, "label")?;
        service::rename_role(store, user, code, label)?;
        return Ok(json!({"code":code,"label":label.trim()}));
    }
    let id = parts.get(2).and_then(|s| s.parse::<i64>().ok()).ok_or_else(|| ApiError::new(404, "找不到 API"))?;
    match (method, parts.as_slice()) {
        ("PATCH", ["api", "items", _]) => {
            require(user, "item.manage")?;
            let input = parse(normalize(
                body,
                &["model", "spec", "pack_unit", "supplier_code", "supplier"],
                &["meters_per_box"],
            )?)?;
            service::update_item(store, user, id, &input)?;
            Ok(json!({"id":id,"ok":true}))
        }
        ("DELETE", ["api", "items", _]) => {
            require(user, "item.manage")?;
            service::delete_item(store, user, id)?;
            Ok(json!({"id":id,"deleted":true}))
        }
        ("PATCH", ["api", "lots", _]) => {
            require(user, "lot.edit")?;
            let input = parse(normalize(
                body.clone(),
                &["manufacture_date", "expiry_date", "supplier", "supplier_lot_code", "remark"],
                &[],
            )?)?;
            let posted = service::update_lot(store, user, id, &input)?;
            Ok(json!({"id":id,"changed":body,"posted_draws":posted}))
        }
        ("DELETE", ["api", "lots", _]) => {
            require(user, "lot.delete")?;
            service::delete_lot(store, user, id)?;
            Ok(json!({"id":id,"deleted":true}))
        }
        ("PATCH", ["api", "users", _]) => {
            require(user, "user.manage")?;
            service::update_user(store, user, id, &parse(normalize(body, &["title"], &[])?)?)?;
            Ok(json!({"id":id,"ok":true}))
        }
        ("POST", ["api", "scans", _, "override"]) => {
            require(user, "scan.override")?;
            service::override_scan(store, user, id, text(&body, "reason")?)?;
            Ok(json!({"id":id,"status":"overridden"}))
        }
        ("PATCH", ["api", "scans", _, "detail"]) => {
            require(user, "issue.detail")?;
            let fields = parse(body.get("field_values").cloned().unwrap_or(Value::Null))?;
            service::update_scan_detail(store, user, id, &fields)?;
            Ok(json!({"ok":true}))
        }
        ("DELETE", ["api", "dictionary", _]) => {
            require(user, "dictionary.manage")?;
            let value: String = store.conn.query_row("SELECT value FROM dictionary WHERE id=?", [id], |r| r.get(0))?;
            service::delete_entry(store, user, id)?;
            Ok(json!({"id":id,"deleted":true,"value":value}))
        }
        ("PATCH", ["api", "dictionary", _]) => {
            require(user, "dictionary.manage")?;
            let body = normalize(body, &[], &[])?;
            if let Some(v) = body.get("value").and_then(Value::as_str) {
                service::rename_entry(store, user, id, v)?;
            }
            if let Some(v) = body.get("active").and_then(Value::as_bool) {
                service::set_entry_active(store, user, id, v)?;
            }
            Ok(json!({"id":id,"ok":true}))
        }
        _ => Err(ApiError::new(404, "找不到 API")),
    }
}

fn scan_list(store: &Store, query: &HashMap<String, String>) -> Result<Value> {
    let all = service::items(&store.conn, service::ItemOrder::Sort)?;
    let max = limit(query, 100)?;
    let rows =
        service::scans_filtered(&store.conn, query_id(query, "lot_id")?, query.get("status").map(String::as_str), max)?;
    Ok(json!(
        rows.iter()
            .map(|s| {
                let mut v = serde_json::to_value(s).unwrap();
                v["detail_pending"] = json!(i32::from(s.detail_pending));
                let item = all.iter().find(|i| Some(i.id) == s.item_id);
                v["item_name"] = json!(item.map(|i| &i.name));
                v["item_model"] = json!(item.and_then(|i| i.model.as_deref()));
                v
            })
            .collect::<Vec<_>>()
    ))
}
fn alerts(store: &Store) -> Result<Value> {
    let a = service::alerts(&store.conn)?;
    let lots = service::lots(&store.conn, None)?;
    let map_lot = |alert: &service::LotAlert| {
        let mut v =
            lots.iter().find(|l| l.id == alert.lot_id).map(lot_json).unwrap_or_else(|| json!({"id":alert.lot_id}));
        v["name"] = json!(alert.name);
        v["expires_on"] = json!(alert.expiry_date);
        v["age_days"] = json!(alert.age_days);
        v
    };
    let pending = a
        .pending_detail
        .iter()
        .map(|s| {
            let item: Option<i64> =
                store
                    .conn
                    .query_row("SELECT item_id FROM material_usage_scan WHERE id = ?", [s.scan_id], |r| r.get(0))?;
            Ok(json!({"id":s.scan_id,"item_id":item,"captured_at":s.captured_at}))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"checked_at":a.checked_at,"total":a.total(),"thresholds":a.thresholds,
        "expiring":a.expiring.iter().map(map_lot).collect::<Vec<_>>(),"stale":a.stale.iter().map(map_lot).collect::<Vec<_>>(),"rejected":a.rejected.iter().map(map_lot).collect::<Vec<_>>(),
        "low_stock":a.low_stock.iter().map(|i|json!({"item_id":i.item_id,"item_code":i.model.as_deref().unwrap_or(&i.name),"name":i.name,"model":i.model,"safety_stock":i.safety_stock,"on_hand":i.on_hand})).collect::<Vec<_>>(),
        "pending_detail":pending }))
}
fn camera_error(error: urdwms_core::camera::CameraError) -> ApiError {
    match error {
        urdwms_core::camera::CameraError::Stale { message, age_seconds, source_time } => ApiError {
            status: 409,
            detail: json!({"code":"stale","message":message,"age_seconds":age_seconds,"source_time":source_time}),
        },
        e => ApiError::new(502, e.to_string()),
    }
}
fn camera_test(store: &Store) -> Result<Value> {
    let c = service::camera_config(&store.conn)?;
    let start = std::time::Instant::now();
    Ok(match urdwms_core::camera::capture(&c) {
        Ok(f) => {
            let mut v = json!({"ok":true,"endpoint":c.endpoint(),"bytes":f.data.len(),"elapsed_ms":start.elapsed().as_millis()});
            if let Some(s) = f.source {
                v["source_name"] = json!(s.name);
                v["source_age_seconds"] = json!(s.age_seconds);
                v["source_time"] = json!(s.time);
            }
            v
        }
        Err(e) => json!({"ok":false,"endpoint":c.endpoint(),"error":e.to_string()}),
    })
}
fn camera_latest(store: &Store) -> Result<Value> {
    let c = service::camera_config(&store.conn)?;
    if !c.enabled || c.transport != "folder" {
        return Ok(json!({"ok":false,"watching":false}));
    }
    Ok(match urdwms_core::camera::newest_image(&c) {
        Ok((_, s)) => {
            json!({"ok":true,"watching":true,"source_name":s.name,"source_age_seconds":s.age_seconds,"source_time":s.time,"fresh":s.age_seconds<=urdwms_core::camera::STALE_AFTER})
        }
        Err(e) => json!({"ok":false,"watching":true,"error":e.to_string()}),
    })
}
fn camera_capture(store: &Store) -> Result<Value> {
    let c = service::camera_config(&store.conn)?;
    if !c.enabled {
        return Err(ApiError::new(400, "影像來源未啟用，請在系統設定開啟"));
    }
    let frame = urdwms_core::camera::capture(&c).map_err(camera_error)?;
    let provider = service::provider(&store.conn);
    let p = service::recognise_and_store(store, provider.as_ref(), &frame.data, "-cam")?;
    let mut v = proposal_json(store, &p)?;
    if let Some(s) = frame.source {
        v["source_name"] = json!(s.name);
        v["source_age_seconds"] = json!(s.age_seconds);
        v["source_time"] = json!(s.time);
    }
    Ok(v)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::testing::{session, store};
    #[test]
    fn browser_stock_workflow_preserves_units_and_permissions() {
        let s = store();
        let admin = session(&s, "peter");
        let mut worker = admin.clone();
        worker.role = "user".into();
        let q = HashMap::new();
        let item = json!({"name":"API film","meters_per_box":100,"has_expiry":false});
        assert_eq!(dispatch(&s, &worker, "POST", "/api/items", &q, item.clone()).unwrap_err().status, 403);
        let id = dispatch(&s, &admin, "POST", "/api/items", &q, item).unwrap()["id"].as_i64().unwrap();
        let received = dispatch(
            &s,
            &admin,
            "POST",
            "/api/lots",
            &q,
            json!({"item_id":id,"receipt_date":"2026-10-27","qty_meters":200,"entered_unit":"米"}),
        )
        .unwrap();
        assert_eq!(received["qty"], 2);
        let lots =
            dispatch(&s, &admin, "GET", "/api/lots", &HashMap::from([("item_id".into(), id.to_string())]), Value::Null)
                .unwrap();
        assert_eq!(lots[0]["item_has_expiry"], 0);
        assert_eq!(lots[0]["lot_state"], "未動用");
        let scan =
            dispatch(&s, &worker, "POST", "/api/scans", &q, json!({"item_id":id,"lot_id":received["id"],"qty":1,"fields":{"產品名稱":"test product","包裝機台":"A"},"detail_pending":false}))
                .unwrap();
        assert_eq!(scan["status"], "posted");
        let records = dispatch(
            &s,
            &worker,
            "GET",
            "/api/scans",
            &HashMap::from([("status".into(), "posted".into()), ("limit".into(), "1".into())]),
            Value::Null,
        )
        .unwrap();
        assert_eq!(records.as_array().unwrap().len(), 1);
        assert_eq!(records[0]["field_values"]["產品名稱"], "test product");
        assert_eq!(records[0]["detail_pending"], 0);
        let items = dispatch(&s, &worker, "GET", "/api/items", &q, Value::Null).unwrap();
        let found = items.as_array().unwrap().iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(found["on_hand"], 1);
        assert_eq!(found["on_hand_m"], 100);
    }
    #[test]
    fn scan_rejects_a_lot_owned_by_another_item() {
        let s = store();
        let admin = session(&s, "peter");
        let q = HashMap::new();
        let all = service::lots(&s.conn, None).unwrap();
        let lot = &all[0];
        let other = service::items(&s.conn, service::ItemOrder::Sort)
            .unwrap()
            .into_iter()
            .find(|i| i.id != lot.item_id)
            .unwrap();
        assert!(
            dispatch(&s, &admin, "POST", "/api/scans", &q, json!({"item_id":other.id,"lot_id":lot.id,"qty":1}))
                .is_err()
        );
    }
    #[test]
    fn proposal_contains_browser_lot_and_recognition_fields() {
        let s = store();
        let admin = session(&s, "peter");
        let q = HashMap::new();
        let id = service::items(&s.conn, service::ItemOrder::Sort)
            .unwrap()
            .into_iter()
            .find(|i| i.model.as_deref() == Some("T7320BC"))
            .unwrap()
            .id;
        let p = dispatch(
            &s,
            &admin,
            "POST",
            "/api/resolve-item",
            &q,
            json!({"item_id":id,"ocr_manufacture_date":"26-Sep-25","ocr_receipt_date":"2026-03-26"}),
        )
        .unwrap();
        assert_eq!(p["display_dates"]["manufacture"], "2025-09-26");
        assert_eq!(p["display_dates"]["receipt"], "2026-03-26");
        assert_eq!(p["recognition"]["manufacture_date"], "26-Sep-25");
        assert!(!p["manufacture_candidate_ids"].as_array().unwrap().is_empty());
        assert_eq!(p["decision"], "lock");
        assert_eq!(p["fifo_ok"], true);
        assert_eq!(p["item_match"]["matched_on"], "manual");
        assert!(p["candidates"][0]["lot_id"].is_i64());
        assert!(p["recognition"]["confidence"].is_number());
        assert!(p["catalogue"][0]["recognisable"].is_boolean());
        let lot_id = p["locked_lot"]["lot_id"].as_i64().unwrap();
        dispatch(&s, &admin, "PATCH", &format!("/api/lots/{lot_id}"), &q, json!({"supplier_lot_code":"corrected"}))
            .unwrap();
        let lot = service::lots(&s.conn, Some(id)).unwrap().into_iter().find(|l| l.id == lot_id).unwrap();
        assert_eq!(lot.supplier_lot_code.as_deref(), Some("corrected"));
    }
    #[test]
    fn signers_use_the_browser_object_contract() {
        let s = store();
        let admin = session(&s, "peter");
        let result = dispatch(&s, &admin, "GET", "/api/auth/signers", &HashMap::new(), Value::Null).unwrap();
        assert!(result[0]["name"].is_string());
        assert!(result[0]["role_label"].is_string());
        assert!(result[0].get("password_hash").is_none());
    }
    #[test]
    fn camera_metadata_never_contains_password() {
        let s = store();
        let admin = session(&s, "peter");
        let q = HashMap::new();
        dispatch(&s, &admin, "PUT", "/api/camera", &q, json!({"password":"private-camera-password"})).unwrap();
        let result = dispatch(&s, &admin, "GET", "/api/camera", &q, Value::Null).unwrap();
        assert!(result.get("password").is_none());
        assert_eq!(result["has_password"], true);
    }
    #[test]
    fn nullable_patch_clears_optional_master_fields() {
        let s = store();
        let admin = session(&s, "peter");
        let q = HashMap::new();
        let id = dispatch(
            &s,
            &admin,
            "POST",
            "/api/items",
            &q,
            json!({"name":"clear me","model":"API-model","meters_per_box":100}),
        )
        .unwrap()["id"]
            .as_i64()
            .unwrap();
        dispatch(&s, &admin, "PATCH", &format!("/api/items/{id}"), &q, json!({"model":null,"meters_per_box":null}))
            .unwrap();
        let items = dispatch(&s, &admin, "GET", "/api/items", &q, Value::Null).unwrap();
        let item = items.as_array().unwrap().iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(item["model"], Value::Null);
        assert_eq!(item["meters_per_box"], Value::Null);
    }
}
