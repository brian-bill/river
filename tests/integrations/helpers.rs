#![allow(dead_code)]

//! Seed formula helpers for exact-match assertions.
//!
//! These mirror the deterministic formulas in `infra/seed.py` so tests can
//! compute expected values without hitting the database.

pub const ROWS: usize = 10_000;

pub const FIRST_NAMES: &[&str] = &[
    "Alice", "Bob", "Carol", "Dave", "Eve", "Frank", "Grace", "Henry",
    "Iris", "Jack", "Kate", "Liam", "Mia", "Noah", "Olivia", "Paul",
    "Quinn", "Rose", "Sam", "Tina", "Uma", "Vince", "Wendy", "Xander",
    "Yuki", "Zara",
];

pub const LAST_NAMES: &[&str] = &[
    "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller",
    "Davis", "Rodriguez", "Martinez", "Hernandez", "Lopez", "Wilson",
    "Anderson", "Thomas", "Taylor", "Moore", "Jackson", "Martin", "Lee",
    "Perez", "Thompson", "White", "Harris", "Sanchez", "Clark",
];

pub const DEPARTMENTS: &[&str] = &[
    "Engineering", "Sales", "Marketing", "Support", "Finance",
    "HR", "Legal", "Product", "Design", "Operations",
];

pub const USER_STATUSES: &[&str] = &["active", "inactive", "suspended", "pending"];

pub const CATEGORIES: &[&str] = &[
    "Electronics", "Clothing", "Books", "Home", "Sports",
    "Food", "Toys", "Health", "Automotive", "Garden",
];

pub const PRODUCT_ADJ: &[&str] = &[
    "Premium", "Basic", "Pro", "Ultra", "Mini",
    "Super", "Mega", "Elite", "Nano", "Max",
];

pub const PRODUCT_NOUN: &[&str] = &[
    "Widget", "Gadget", "Device", "Tool", "Kit",
    "Pack", "Set", "System", "Module", "Unit",
];

pub const ORDER_STATUSES: &[&str] = &[
    "pending", "paid", "shipped", "delivered", "cancelled", "refunded",
];

pub fn user_name(i: usize) -> String {
    let fn_idx = (i * 7 + 3) % FIRST_NAMES.len();
    let ln_idx = (i * 13 + 5) % LAST_NAMES.len();
    format!("{} {}", FIRST_NAMES[fn_idx], LAST_NAMES[ln_idx])
}

pub fn user_email(i: usize) -> String {
    let fn_idx = (i * 7 + 3) % FIRST_NAMES.len();
    let ln_idx = (i * 13 + 5) % LAST_NAMES.len();
    format!(
        "{}.{}{}@example.com",
        FIRST_NAMES[fn_idx].to_lowercase(),
        LAST_NAMES[ln_idx].to_lowercase(),
        i
    )
}

pub fn user_department(i: usize) -> &'static str {
    DEPARTMENTS[(i * 11 + 2) % DEPARTMENTS.len()]
}

pub fn user_salary(i: usize) -> i64 {
    35000 + ((i as i64) * 17) % 115000
}

pub fn user_status(i: usize) -> &'static str {
    USER_STATUSES[(i * 3) % USER_STATUSES.len()]
}

pub fn user_is_verified(i: usize) -> bool {
    i.is_multiple_of(3)
}

pub fn product_name(i: usize) -> String {
    let adj = PRODUCT_ADJ[(i * 7) % PRODUCT_ADJ.len()];
    let noun = PRODUCT_NOUN[(i * 3) % PRODUCT_NOUN.len()];
    format!("{} {} {}", adj, noun, i)
}

pub fn product_category(i: usize) -> &'static str {
    CATEGORIES[(i * 11) % CATEGORIES.len()]
}

pub fn product_price(i: usize) -> f64 {
    (0.99 * 100.0 + ((i as f64) * 31.0) % 99901.0).round() / 100.0
}

pub fn product_stock(i: usize) -> i64 {
    ((i * 7) % 500) as i64
}

pub fn product_rating(i: usize) -> f64 {
    (100.0 + ((i as f64) * 13.0) % 400.0).round() / 100.0
}

pub fn product_is_active(i: usize) -> bool {
    !i.is_multiple_of(10)
}

pub fn order_user_id(i: usize) -> i64 {
    ((i * 7 + 1) % ROWS + 1) as i64
}

pub fn order_status(i: usize) -> &'static str {
    ORDER_STATUSES[(i * 5) % ORDER_STATUSES.len()]
}

pub fn order_total(i: usize) -> f64 {
    (500.0 + ((i as f64) * 43.0) % 50000.0).round() / 100.0
}

pub fn order_item_order_id(i: usize) -> i64 {
    ((i * 3 + 1) % ROWS + 1) as i64
}

pub fn order_item_product_id(i: usize) -> i64 {
    ((i * 11 + 2) % ROWS + 1) as i64
}

pub fn order_item_quantity(i: usize) -> i64 {
    (1 + (i * 7) % 10) as i64
}

pub fn order_item_unit_price(i: usize) -> f64 {
    (0.99 * 100.0 + ((i as f64) * 31.0) % 99901.0).round() / 100.0
}

/// Count users matching a given status.
pub fn count_users_with_status(status: &str) -> usize {
    (0..ROWS).filter(|&i| user_status(i) == status).count()
}

/// Count users in a given department.
pub fn count_users_in_department(dept: &str) -> usize {
    (0..ROWS).filter(|&i| user_department(i) == dept).count()
}

/// Count products in a given category.
pub fn count_products_in_category(cat: &str) -> usize {
    (0..ROWS).filter(|&i| product_category(i) == cat).count()
}

/// Count active products.
pub fn count_active_products() -> usize {
    (0..ROWS).filter(|&i| product_is_active(i)).count()
}

/// Count orders with a given status.
pub fn count_orders_with_status(status: &str) -> usize {
    (0..ROWS).filter(|&i| order_status(i) == status).count()
}
