# Docker image

We use a docker image in CI to have a reproducible basis and save setup time.
If you need new dependencies, rebuild the image, push it and update the image in the `ci.yml`.

The image is built and pushed like this:

```sh
cd docker

docker build -t sgasse/ubuntu_builder:0.0.2 .

docker login
docker push sgasse/ubuntu_builder:0.0.2
```
