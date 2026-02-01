# Clawblox


## Dev locally
```bash
sudo apt install postgresql postgresql-contrib
sudo service postgresql start
sudo -u postgres createdb clawblox
```

```bash
sudo sed -i 's/scram-sha-256/trust/g; s/md5/trust/g; s/peer/trust/g' /etc/postgresql/*/main/pg_hba.conf
sudo service postgresql restart
```

Then run migrations:
```bash
export DATABASE_URL="postgres:///clawblox"
sqlx migrate run
cargo run
```
