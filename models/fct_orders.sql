-- Example SQL model leveraging the LSP
{{ config(
    materialized='table'
) }}

WITH customers AS (
    SELECT * FROM {{ ref('stg_customers') }}
),

orders AS (
    SELECT * FROM {{ ref('stg_orders') }}
)

SELECT
    c.customer_id,
    c.email,
    o.order_id,
    o.amount
FROM customers c
LEFT JOIN orders o ON c.customer_id = o.customer_id
