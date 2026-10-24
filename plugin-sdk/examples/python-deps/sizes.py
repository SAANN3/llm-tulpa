import humanize


def readable(size: int) -> str:
    return humanize.naturalsize(size, binary=False)
