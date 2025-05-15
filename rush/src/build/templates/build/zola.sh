cd {{ location }} || exit
{% for d,v in domains -%}
export DOMAIN_{{ d | envname }}="{{ v }}"
{% endfor %}
{% for d,v in env -%}
export {{ d | envname }}="{{ v }}"
{% endfor %}

if [ -f "tailwind.config.js" ]; then
    tailwindcss -i styles/main.css -o static/styles/main.css
fi
export COMPOENT_DOMAIN="{{ domain }}"
if [[ "$COMPOENT_DOMAIN" == *"localhost"* ]]; then
    export COMPOENT_PROTOCOL="http"
    export BASE_URL="$COMPOENT_PROTOCOL://$COMPOENT_DOMAIN:9000" # TODO: Get the port from somewhere

else
    export COMPOENT_PROTOCOL="https"
    export BASE_URL="$COMPOENT_PROTOCOL://$COMPOENT_DOMAIN"

fi

zola build --output-dir ./dist --base-url ${BASE_URL} --force
