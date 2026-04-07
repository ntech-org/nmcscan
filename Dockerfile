# Legacy Dockerfile - DEPRECATED
# This Dockerfile is no longer used. Please use the service-specific Dockerfiles:
# - packages/api/Dockerfile
# - packages/scanner/Dockerfile

FROM alpine:latest
RUN echo "ERROR: This Dockerfile is deprecated. Use packages/api/Dockerfile or packages/scanner/Dockerfile" && exit 1
