FROM nginx:alpine
WORKDIR /usr/share/nginx/html
COPY ./dist/nginx.conf /etc/nginx/nginx.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
