// Copyright 2020 Joyent, Inc.

use serde_json::Error as SerdeError;
use serde_json::Value;
use slog::{debug, error, Logger};

use cueball_postgres_connection::PostgresConnection;
use fast_rpc::protocol::{FastMessage, FastMessageData};

use crate::bucket::{
    bucket_not_found, response, to_json, BucketResponse, GetBucketPayload,
};
use crate::error::BucketsMdapiError;
use crate::metrics::RegisteredMetrics;
use crate::sql;
use crate::types::HandlerResponse;
use crate::util::array_wrap;

pub(crate) fn decode_msg(
    value: &Value,
) -> Result<Vec<GetBucketPayload>, SerdeError> {
    serde_json::from_value::<Vec<GetBucketPayload>>(value.clone())
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn action(
    msg_id: u32,
    method: &str,
    metrics: &RegisteredMetrics,
    log: &Logger,
    payload: GetBucketPayload,
    conn: &mut PostgresConnection,
) -> Result<HandlerResponse, String> {
    // Make database request
    do_get(method, &payload, conn, metrics, log)
        .and_then(|maybe_resp| {
            // Handle the successful database response
            debug!(log, "operation successful");
            let value = match maybe_resp {
                Some(resp) => to_json(resp),
                None => bucket_not_found(),
            };
            let msg_data =
                FastMessageData::new(method.into(), array_wrap(value));
            let msg: HandlerResponse =
                FastMessage::data(msg_id, msg_data).into();
            Ok(msg)
        })
        .or_else(|e| {
            // Handle database error response
            error!(log, "operation failed"; "error" => &e);

            // Database errors are returned to as regular Fast messages
            // to be handled by the calling application
            let err = BucketsMdapiError::PostgresError(e);
            let msg_data = FastMessageData::new(
                method.into(),
                array_wrap(err.into_fast()),
            );
            let msg: HandlerResponse =
                FastMessage::data(msg_id, msg_data).into();
            Ok(msg)
        })
}

fn do_get(
    method: &str,
    payload: &GetBucketPayload,
    mut conn: &mut PostgresConnection,
    metrics: &RegisteredMetrics,
    log: &Logger,
) -> Result<Option<BucketResponse>, String> {
    let sql = get_sql(payload.vnode);

    sql::query(
        sql::Method::BucketGet,
        &mut conn,
        sql.as_str(),
        &[&payload.owner, &payload.name],
        metrics,
        log,
    )
    .map_err(|e| e.to_string())
    .and_then(|rows| response(method, &rows))
}

fn get_sql(vnode: u64) -> String {
    [
        "SELECT id, owner, name, created \
         FROM manta_bucket_",
        &vnode.to_string(),
        &".manta_bucket WHERE owner = $1 \
          AND name = $2",
    ]
    .concat()
}
