def leaf(value):
    return value


def outer(value=leaf(0)):
    # phantom(value) should never be a reference
    message = "phantom leaf outer"
    leaf(value); leaf(value)
    service.save(value)

    def inner():
        return leaf(value)

    callback = lambda: leaf(value)
    return inner()


class Worker:
    def save(self, value):
        return leaf(value)


async def fetch():
    return await service.load()


leaf(9)
