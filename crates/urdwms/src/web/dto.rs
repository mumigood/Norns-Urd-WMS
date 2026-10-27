//! Adapt service models to the browser's established JSON contract.
use crate::{
    service::{self, Item, Lot, Proposal, Store},
    web::ApiError,
};
use serde_json::{Value, json};

pub fn item_json(item: &Item) -> Value {
    let mut value = serde_json::to_value(item).expect("item serialization");
    value["label"] = json!(item.label());
    value["matchable"] = json!(item.matchable());
    value["recognisable"] = json!(item.recognisable());
    value["on_hand_m"] = json!(item.on_hand_m());
    value["has_expiry"] = json!(i32::from(item.has_expiry));
    value["use_recognition"] = json!(i32::from(item.use_recognition));
    value
}
pub fn lot_json(lot: &Lot) -> Value {
    let mut value = serde_json::to_value(lot).expect("lot serialization");
    value["item_label"] = json!(lot.item_label());
    value["lot_state"] = json!(lot.state());
    value["item_has_expiry"] = json!(i32::from(lot.item_has_expiry));
    value["effective_expiry"] = json!(lot.expiry_date);
    value
}
pub fn proposal_json(store: &Store, p: &Proposal) -> Result<Value, ApiError> {
    let master = service::items(&store.conn, service::ItemOrder::Sort)?;
    let catalogue: Vec<_> = master.iter().map(item_json).collect();
    let pool = if let Some(id) = p.item_id { service::lots(&store.conn, Some(id))? } else { vec![] };
    let candidates:Vec<_>=pool.iter().filter(|l|l.drawable()).map(|l|json!({"lot_id":l.id,"receipt_date":l.receipt_date,"manufacture_date":l.manufacture_date,"qty_on_hand":l.qty_on_hand})).collect();
    let locked=p.locked_lot.as_ref().map(|l|json!({"lot_id":l.lot_id,"receipt_date":l.receipt_date,"manufacture_date":l.manufacture_date,"qty_on_hand":l.qty_on_hand}));
    let expected = master.iter().find(|i| Some(i.id) == p.item_id).and_then(|i| i.supplier_code.as_deref());
    Ok(
        json!({"image_path":p.image_path,"recognition":{"model_code":p.recognition.model_code,"item_code":p.recognition.item_code,"manufacture_date":p.recognition.manufacture_date,"receipt_date":p.recognition.receipt_date,"confidence":p.recognition.receipt_date_confidence,"item_code_confidence":p.recognition.item_code_confidence,"stamp_visible":p.recognition.stamp_visible,"notes":p.recognition.notes,"error":p.recognition.error},"timing_ms":{"model":p.timing_ms},
        "source_name":p.source_name,"source_time":p.source_time,"item_id":p.item_id,"item_name":p.item_name,"item_model":p.item_model,"item_label":p.item_label(),
        "item_match":{"decision":if p.item_id.is_some(){"lock"}else{"defer"},"item_id":p.item_id,"matched_on":p.item_matched_on,"reason":p.item_defer_reason,"contenders":p.item_contenders},
        "catalogue":catalogue,"candidates":candidates,"expected_supplier_code":expected,"decision":p.decision,"defer_reason":p.defer_reason,"match_distance":p.match_distance,
        "lot_matched_on":p.lot_matched_on,"locked_lot":locked,"fifo_ok":p.fifo_ok,"fifo_expected_date":p.fifo_expected_date,"fifo_target_lot_id":p.fifo_target_lot_id,"fifo_basis":p.fifo_basis}),
    )
}
