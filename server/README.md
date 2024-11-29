# rtherm-server

## Grafana

+ Connect to `localhost:4101`
+ Enter `admin/admin`
+ Change password and optinally username

+ Go to Data sources and add a new one
+ Add PostgreSQL datasource
  + Name: `postgresql`
  + Url: `postgres`
  + Database name: `rtherm`
  + Username: `rtherm`
  + Password: `rtherm`
  + TLS/SSL Mode: `disable`

+ Go to Dashboards and create a new one
  + Data source: `postgresql`
  + Format: Time series
  + Columns:
    + `value`
    + `time`
    + `channel_id`
  + Order by: `time`, limit: blank
  + Save
