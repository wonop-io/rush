FROM nginx:alpine
WORKDIR /usr/share/nginx/html
COPY ./rushd/nginx.conf /etc/nginx/nginx.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
