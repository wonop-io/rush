```
brew install 1password-cli
```

https://developer.1password.com/docs/cli/get-started/#install

op://<vault-name>/<item-name>/[section-name/]<field-name>

Example:
op://development/GitHub/credentials/personal_token

Summary:

```
op vault create [product_name]
op item create --title [component_name]-[environment] --vault [product_name] --category "Secure note" [field_name1]=[new_value1] [field_name2]=[new_value2] ...
op item list --vault [product_name]
op item get [item_id] --reveal
op item edit [item_id] --vault [product_name] [field_name1]=[new_value1] [field_name2]=[new_value2] ...
```

Example:

```
$ op vault create logom8.com
$ op item create \
    --title "workers-staging" \
    --vault logom8.com \
    --category "Secure note" \
    aws_access_key_id=AKIAIOSFODNN7EXAMPLE \
    aws_secret_access_key="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
ID:          5hdpkcwi24mzlrecklad7xiuym
Title:       workers-staging
Vault:       logom8.com (qhrlqlarbj25auspnigc7zjeae)
Created:     now
Updated:     now
Favorite:    false
Version:     1
Category:    SECURE_NOTE
Fields:
  aws_access_key_id:        [use 'op item get 5hdpkcwi24mzlrecklad7xiuym --reveal' to reveal]
  aws_secret_access_key:    [use 'op item get 5hdpkcwi24mzlrecklad7xiuym --reveal' to reveal]

$ op item edit 5foagbv4jed6o5doxewupk2fgm --vault logom8.com  \
    aws_access_key_id=NEW_ACCESS_KEY_ID \
    aws_secret_access_key=NEW_SECRET_ACCESS_KEY

$ op item list --vault logom8.com
ID                            TITLE                  VAULT                 EDITED
5foagbv4jed6o5doxewupk2fgm    backend-staging        logom8.com            2 minutes ago
jtcfguaxhck2il5fqy2im7ecbm    frontend-staging        logom8.com            7 minutes ago


$ op item get 5foagbv4jed6o5doxewupk2fgm --reveal
ID:          5foagbv4jed6o5doxewupk2fgm
Title:       backend-staging
Vault:       logom8.com (qhrlqlarbj25auspnigc7zjeae)
Created:     3 minutes ago
Updated:     2 minutes ago
Favorite:    false
Version:     2
Category:    SECURE_NOTE
Fields:
  aws_access_key_id:        NEW_ACCESS_KEY_ID
  aws_secret_access_key:    NEW_SECRET_ACCESS_KEY
```
