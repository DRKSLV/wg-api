use actix_multipart::Multipart;
use actix_web::{ HttpResponse, Responder, get, put, delete, post, http::StatusCode, dev::{ConnectionInfo}, web, Error,};
use rust_decimal::Decimal;
use serde::{Serialize, Deserialize};
use crate::{DB_POOL, change_upload, auth::res_error, file_uploads::{Upload, DBRetrUpload}};

use super::auth::Identity;

use crate::file_uploads::{multipart_parse, AscendingUpload, TempUpload};

// ================================================================================== STATE MODEL ==================================================================================

#[derive(Serialize)]
struct WG {
    id : i32,
    url: String,

    name: String,
    description: String,

    profile_pic: Option<DBRetrUpload>,
    header_pic: Option<DBRetrUpload>
}

#[derive(Serialize)]
struct User {
    id : i32,
    username: String,

    name: String,
    bio: String,

    profile_pic: Option<DBRetrUpload>,
}

#[derive(Serialize, Deserialize)]
struct Cost {
    id: i32,
    wg_id : i32,
    name: String,
    amount: rust_decimal::Decimal,
    creditor_id: i32,
    #[serde(with= "time::serde::rfc3339")]
    added_on: time::OffsetDateTime,

    receit: Option<DBRetrUpload>,
    my_share: Option<DBCostShare>,
    nr_shares: Option<i64>,
    nr_unpaid_shares:  Option<i64>
}

#[derive(sqlx::Type, Serialize, Deserialize)]
pub struct DBCostShare {
    cost_id: Option<i32>, 
    debtor_id: Option<i32>,
    paid: Option<bool>
}

#[derive(Serialize, Deserialize)]
pub struct CostShare {
    cost_id: i32, 
    debtor_id: i32,
    paid: bool
}

#[derive(Serialize, Deserialize)]
struct CostParameter {
    name: String,
    amount: rust_decimal::Decimal,
    #[serde(with= "time::serde::iso8601")]
    added_on: time::OffsetDateTime,
    debtors: Vec<(i32, bool)>
}

struct DebtTableRecord {
    u1: Option<i32>, 
    to_recieve: Option<Decimal>, 
    u2: Option<i32>, 
    to_pay: Option<Decimal> 
}

#[derive(Serialize)]
struct UserDebt {
    user_id: i32,
    to_recieve: Decimal,
    to_pay: Decimal
}

// ================================================================================== ROUTES ==================================================================================
#[get("/me")]
async fn get_user_me(mut identity: Identity) -> impl Responder {
    identity.password_hash = "<Not Provided>".to_string();

    HttpResponse::Ok()
        .json(identity)
}

#[put("/me")]
async fn put_user_me(identity: Identity, payload: Multipart) -> Result<impl Responder, Error> {

    #[derive(Serialize, Default)]
    struct ResJson {
        name: Option<String>,
        bio: Option<String>,
        username: Option<String>,
        profile_pic: Option<Upload>
    }
    let mut res_json = ResJson {
        ..Default::default()
    };

    // Get Multipart Fields
    let mut lmaobozo = multipart_parse(payload, ["name", "bio", "username"], ["profile_pic"]).await?;
    trace!("Bozo fields: {:?}", lmaobozo);

    if let Some(profile_picf) = &mut lmaobozo.1[0] {
        let new_upl = change_upload!("users", "profile_pic", i32)(profile_picf.move_responsibility(), identity.id).await;
        if let Ok (new_upl) = new_upl  {
            res_json.profile_pic = Some(new_upl);
        } else {
            warn!("Couldn't change upload :(");
        }
    }
    if let Some(name) = &lmaobozo.0[0] {
        let res = sqlx::query!("UPDATE users SET name=$1 WHERE id=$2", name, identity.id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.name = Some(name.to_owned());
        }
    }
    if let Some(bio) = &lmaobozo.0[1] {
        let res = sqlx::query!("UPDATE users SET bio=$1 WHERE id=$2", bio, identity.id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.bio = Some(bio.to_owned());
        }
    }
    if let Some(username) = &lmaobozo.0[2] {
        let res = sqlx::query!("UPDATE users SET username=$1 WHERE id=$2", username, identity.id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.username = Some(username.to_owned());
        }
    }
    
    Ok(HttpResponse::Ok()
        .json(res_json))
}

// user_change_password, user_revoke_tokens

#[get("/my_wg")]
async fn get_wg(identity: Identity) -> Result<impl Responder, Error> {
    let wgopt =
    if let Some(wg_id)  = identity.wg {
        //let wg = sqlx::query_as!(WG, "SELECT * FROM wgs WHERE id = $1", wg_id)
        let wg : WG = sqlx::query_as!(WG, r#"SELECT wgs.id, url, name, description, 
        (pp.id, pp.extension, pp.original_filename, pp.size_kb) as "profile_pic: DBRetrUpload",
        (hp.id, hp.extension, hp.original_filename, hp.size_kb) as "header_pic: DBRetrUpload"
    FROM wgs 
    LEFT JOIN uploads AS pp ON profile_pic = pp.id
    LEFT JOIN uploads AS hp ON header_pic = hp.id
    WHERE wgs.id = $1"#, wg_id)
            .fetch_one(DB_POOL.get().await).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;
        Some(wg)
    } else {
        None
    };
    
    Ok( HttpResponse::Ok()
    .json(wgopt) )
}

#[put("/my_wg")]
async fn put_wg(identity: Identity, payload: Multipart) -> Result<impl Responder, Error> {
    let wg_id = identity.wg.ok_or_else(|| res_error::<&'static str>(StatusCode::FORBIDDEN, None, "You are not assigned to a WG, and therefore can't edit yours.") )?;

    #[derive(Serialize, Default)]
    struct ResJson {
        name: Option<String>,
        url: Option<String>,
        description: Option<String>,
        profile_pic: Option<Upload>,
        header_pic: Option<Upload>
    }
    let mut res_json = ResJson {
        ..Default::default()
    };

    // Get Multipart Fields
    let mut lmaobozo = multipart_parse(payload, ["name", "url", "description"], ["profile_pic", "header_pic"]).await?;
    trace!("Bozo fields: {:?}", lmaobozo);

    if let Some(profile_picf) = &mut lmaobozo.1[0] {
        let new_upl = change_upload!("wgs", "profile_pic", i32)(profile_picf.move_responsibility(), wg_id).await;
        if let Ok (new_upl) = new_upl  {
            res_json.profile_pic = Some(new_upl);
        } else {
            warn!("Couldn't change upload :(");
        }
    }
    if let Some(header_picf) = &mut lmaobozo.1[1] {
        let new_upl = change_upload!("wgs", "header_pic", i32)(header_picf.move_responsibility(), wg_id).await;
        if let Ok (new_upl) = new_upl  {
            res_json.header_pic = Some(new_upl);
        } else {
            warn!("Couldn't change upload :(");
        }
    }
    if let Some(name) = &lmaobozo.0[0] {
        let res = sqlx::query!("UPDATE wgs SET name=$1 WHERE id=$2", name, wg_id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.name = Some(name.to_owned());
        }
    }
    if let Some(url) = &lmaobozo.0[1] {
        let res = sqlx::query!("UPDATE wgs SET url=$1 WHERE id=$2", url, wg_id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.url = Some(url.to_owned());
        }
    }
    if let Some(description) = &lmaobozo.0[2] {
        let res = sqlx::query!("UPDATE wgs SET description=$1 WHERE id=$2", description, wg_id)
            .execute(DB_POOL.get().await).await;
        if let Ok(_res) = res {
            res_json.description = Some(description.to_owned());
        }
    }
    
    Ok(HttpResponse::Ok()
        .json(res_json))
}

#[get("/my_wg/users")]
async fn get_wg_users(identity: Identity) -> Result<impl Responder, Error>  {
    let wgopt =
    if let Some(wg_id)  = identity.wg {
        //let wg = sqlx::query_as!(WG, "SELECT * FROM wgs WHERE id = $1", wg_id)
        let wg : Vec<User> = sqlx::query_as!(User, r#"SELECT users.id, name, bio, username, 
        (pp.id, pp.extension, pp.original_filename, pp.size_kb) as "profile_pic: DBRetrUpload"
    FROM users 
    LEFT JOIN uploads AS pp ON profile_pic = pp.id
    WHERE users.wg = $1"#, wg_id)
            .fetch_all(DB_POOL.get().await).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;
        Some(wg)
    } else {
        None
    };
    
    Ok( HttpResponse::Ok()
    .json(wgopt) )
}

#[get("/my_wg/costs")]
async fn get_wg_costs(identity: Identity) -> Result<impl Responder, Error> {
    let costs_opt =
    if let Some(wg_id)  = identity.wg {
        let cost = sqlx::query_as!(Cost, r#"
        SELECT costs.id, wg_id, name, amount, creditor_id, (pp.id, pp.extension, pp.original_filename, pp.size_kb) as "receit: DBRetrUpload",
            added_on, ROW(my_share.cost_id, my_share.debtor_id, my_share.paid) as "my_share: DBCostShare",
            count(*) as nr_shares, sum( CASE WHEN shares.paid = false AND shares.debtor_id != creditor_id THEN 1 ELSE 0 END ) as nr_unpaid_shares       
        FROM costs
        LEFT JOIN cost_shares as shares ON costs.id = shares.cost_id -- multiple per row
        LEFT JOIN cost_shares as my_share ON costs.id = my_share.cost_id AND my_share.debtor_id = $1 -- guarranteed to be unique per row, as (cost_id, debtor_id) is PRIMARY
        LEFT JOIN uploads AS pp ON receit_id = pp.id
        WHERE wg_id = $2
        GROUP BY costs.id, my_share.cost_id, my_share.debtor_id, my_share.paid, pp.id, pp.extension, pp.original_filename, pp.size_kb;"#, identity.id, wg_id)
            .fetch_all(DB_POOL.get().await).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;

        Some(cost)
    } else {
        None
    };

    Ok( HttpResponse::Ok()
    .json(costs_opt) )
}

#[post("/my_wg/costs")]
async fn post_wg_costs(identity: Identity, new_cost: web::Json<CostParameter>) -> Result<impl Responder, Error> {
    let wg_id = identity.wg.ok_or(res_error::<&'static str>(StatusCode::FORBIDDEN, None, "You need to be in a wg for this operation."))?;

    let mut trx = DB_POOL.get().await.begin().await
        .map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;

    let cost_id: i32 = sqlx::query_scalar!("INSERT INTO costs (wg_id, name, amount, creditor_id, added_on) VALUES
    ($1, $2, $3, $4, $5) RETURNING id;", wg_id, new_cost.name, new_cost.amount, identity.id, new_cost.added_on)
        .fetch_one(&mut trx).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;
   
    let users = sqlx::query_scalar!("SELECT id FROM users WHERE wg = $1", wg_id)
        .fetch_all(&mut trx).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;
    
    for debtor in new_cost.debtors.iter() {
        if !users.contains( &debtor.0 )  {
            continue;
        }
        let mut paid = debtor.1;
        if identity.id == debtor.0 {
            paid = true;
        }

        sqlx::query_scalar!("INSERT INTO cost_shares (cost_id, debtor_id, paid) VALUES
        ($1, $2, $3);", cost_id, debtor.0, paid)
            .execute(&mut trx).await.map_err(|e| {error!("OAHo: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;
    }
    trx.commit().await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;

    Ok( HttpResponse::Ok()
        .json(cost_id) )
}

#[get("/my_wg/costs/{id}/shares")]
async fn get_wg_costs_id(identity: Identity, params: web::Path<(i32,)>) -> Result<impl Responder, Error> {
    let shares_opt =
    if let Some(wg_id)  = identity.wg {
        let shares = sqlx::query_as!(CostShare, "SELECT cost_id, debtor_id, paid 
        FROM cost_shares LEFT JOIN costs ON cost_id = costs.id
        WHERE cost_id=$1 AND costs.wg_id = $2", params.0, wg_id)
            .fetch_all(DB_POOL.get().await).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;

        Some(shares)
    } else {
        None
    };

    Ok( HttpResponse::Ok()
    .json(shares_opt) )
}

#[put("/my_wg/costs/{id}")]
async fn put_wg_costs_id(identity: Identity) -> impl Responder {
    todo!();
    ""
}

#[delete("/my_wg/costs/{id}")]
async fn delete_wg_costs_id(identity: Identity) -> impl Responder {
    todo!();
    ""
}



#[get("/my_wg/costs/stats")]
async fn get_wg_costs_stats(identity: Identity) -> Result<impl Responder, Error> {
    let costs_opt =
    if let Some(wg_id)  = identity.wg {
        let dtrs: Vec<DebtTableRecord> = sqlx::query_as!( DebtTableRecord , r#"
            WITH debt_table AS (
                SELECT debtor_id, creditor_id, (amount/nr_shares)::NUMERIC(16,2) as owed
                FROM cost_shares
                LEFT JOIN (
                    SELECT costs.id, amount, creditor_id, wg_id,
                        count(*) as nr_shares, sum( CASE WHEN shares.paid = false AND shares.debtor_id != creditor_id THEN 1 ELSE 0 END ) as nr_unpaid_shares
                    FROM costs
                    LEFT JOIN cost_shares as shares ON costs.id = shares.cost_id -- multiple per row
                    GROUP BY costs.id
                ) AS cost_agg ON cost_agg.id = cost_shares.cost_id
                WHERE debtor_id != creditor_id AND paid = false AND cost_agg.wg_id = $1
            ), recieve_table AS (
                SELECT creditor_id as user_id, sum(owed) as to_recieve
                FROM debt_table
                GROUP BY creditor_id
            ), pay_table AS (
                SELECT debtor_id as user_id, sum(owed) as to_pay
                FROM debt_table
                GROUP BY debtor_id
            )
            SELECT recieve_table.user_id as u1, to_recieve, pay_table.user_id as u2, to_pay FROM recieve_table
            FULL OUTER JOIN pay_table ON( recieve_table.user_id = pay_table.user_id );"#
        , wg_id)
            .fetch_all(DB_POOL.get().await).await.map_err(|e| {error!("AHH: {}", e); res_error(StatusCode::INTERNAL_SERVER_ERROR, Some(e), "Database quirked up, sry :(")})?;

        let mut debts: Vec<UserDebt> = vec![];
        for record in dtrs {
            let user_id = record.u1.or(record.u2);
            if let Some (user_id) = user_id {
                debts.push(UserDebt {
                    user_id,
                    to_recieve: record.to_recieve.unwrap_or(Decimal::ZERO),
                    to_pay:  record.to_pay.unwrap_or(Decimal::ZERO)
                })
            }
        }

        Some(debts)
    } else {
        None
    };

    Ok( HttpResponse::Ok()
    .json(costs_opt) )
}


pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            //.wrap(Authentication)
            .service(get_user_me)
            .service(put_user_me)
            .service(get_wg)
            .service(put_wg)
            .service(get_wg_users)
            .service(get_wg_costs)
            .service(post_wg_costs)
            .service(get_wg_costs_stats)

            .service(get_wg_costs_id)
            .service(put_wg_costs_id)
            .service(delete_wg_costs_id)

    );
}

/*
 let filepath=format!("uploads/temp/{}{}", temp_upload.local_id, match get_mime_extensions(field.content_type()) {
            Some(ext) => {
                let mut str = ".".to_string();
                str.push_str( ext.first().map(|s| *s).unwrap_or("cringe") );
                str
            },
            None=>"".to_string()
        } );
 */