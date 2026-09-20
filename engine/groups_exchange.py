"""Logical CDXML groups, independent of the chemistry graph and graphic paint."""
import xml.etree.ElementTree as ET


def read_groups(root, object_map, first_id):
    def members(el):
        if el in object_map:
            return object_map[el]
        return [id for child in el for id in members(child)]
    result = []
    seen = {}
    # Innermost groups first. Redundant wrappers carry no extra membership.
    for el in reversed(list(root.iter("group"))):
        ids = sorted(set(members(el)))
        if len(ids) < 2:
            continue
        key = tuple(ids)
        integral = el.get("Integral", "no") == "yes"
        if key in seen:
            seen[key]["integral"] |= integral
            continue
        group = dict(id=first_id + len(result), members=ids, integral=integral)
        result.append(group)
        seen[key] = group
    return result


def write_groups(page, doc, fragment, object_map, atom_xml_ids, next_id):
    groups = doc.get("groups", [])
    if not groups:
        return next_id
    # A molecule must remain one chemical fragment inside its owning group.
    for group in groups:
        members = set(group["members"])
        if any((b["a"] in members) != (b["b"] in members) for b in doc["bonds"]):
            raise ValueError("A group cuts through a molecule; ungroup or group the whole molecule before CDXML export")
    remaining = {a["id"] for a in doc["atoms"]}
    components = []
    while remaining:
        component = {min(remaining)}
        while True:
            previous = len(component)
            for bond in doc["bonds"]:
                if bond["a"] in component or bond["b"] in component:
                    component.update((bond["a"], bond["b"]))
            if previous == len(component):
                break
        remaining -= component
        components.append(component)
    children = list(fragment)
    for index, component in enumerate(components):
        if index == 0:
            target = fragment
        else:
            target = ET.SubElement(page, "fragment", id=str(next_id), Z=fragment.get("Z", "0"))
            next_id += 1
        xml_ids = {str(atom_xml_ids[id]) for id in component}
        for el in children:
            belongs = (el.tag == "n" and el.get("id") in xml_ids) or (el.tag == "b" and el.get("B") in xml_ids) or (el.tag == "graphic" and any(rep.get("object") in xml_ids for rep in el.findall("represent")))
            if belongs and target is not fragment:
                fragment.remove(el)
                target.append(el)
        for id in component:
            object_map[id] = target
    containers = {}
    for group in groups:
        containers[group["id"]] = ET.Element("group", id=str(next_id), Integral="yes" if group.get("integral") else "no")
        next_id += 1
    for group in groups:
        supersets = [other for other in groups if len(other["members"]) > len(group["members"])
                     and set(group["members"]).issubset(other["members"])]
        parent = containers[min(supersets, key=lambda g: len(g["members"]))["id"]] if supersets else page
        parent.append(containers[group["id"]])
    moved = set()
    for id, el in object_map.items():
        owners = [g for g in groups if id in g["members"]]
        if not owners or el in moved:
            continue
        owner = min(owners, key=lambda g: len(g["members"]))
        page.remove(el)
        containers[owner["id"]].append(el)
        moved.add(el)
    for container in containers.values():
        values = [int(el.get("Z")) for el in container.iter() if "Z" in el.attrib]
        container.set("Z", str(min(values, default=0)))
    return next_id
