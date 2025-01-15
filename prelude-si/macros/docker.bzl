load(
    "@prelude-si//:docker.bzl",
    _docker_image = "docker_image",
    _docker_image_promote = "docker_image_promote",
    _docker_image_release = "docker_image_release",
)

def docker_image(
        name,
        dockerfile = "Dockerfile",
        organization = "systeminit",
        source_url = "http://github.com/systeminit/si.git",
        author = "The System Initiative <dev@systeminit.com>",
        license = "Apache-2.0",
        visibility = ["PUBLIC"],
        release_target = "release",
        promote_target = "promote",
        promote_multi_arches = [],
        **kwargs):
    _docker_image(
        name = name,
        dockerfile = dockerfile,
        organization = organization,
        source_url = source_url,
        author = author,
        license = license,
        visibility = visibility,
        **kwargs,
    )

    _docker_image_release(
        name = release_target,
        docker_image = ":{}".format(name),
        visibility = visibility,
    )

    _docker_image_promote(
        name = promote_target,
        image_name = "{}/{}".format(organization, kwargs.get("image_name", name)),
        multi_arches = promote_multi_arches,
        visibility = visibility,
    )
