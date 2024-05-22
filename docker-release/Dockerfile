FROM alpine AS setup
RUN adduser -D -H applehealthimport
COPY ./epoll-test /usr/local/bin/
RUN chmod 755 /usr/local/bin/epoll-test

FROM scratch

COPY --from=setup /etc/passwd /etc/passwd
COPY --from=setup /usr/local/bin/epoll-test /usr/local/bin/epoll-test
USER applehealthimport

CMD ["/usr/local/bin/epoll-test"]
