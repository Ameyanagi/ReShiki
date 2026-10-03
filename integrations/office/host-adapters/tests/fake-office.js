// A queued Office.js double: reads need load + sync, writes are deferred, and
// sync can fail after an applied operation. It does not model Office rendering.
export function fakeOffice(host) {
  let serial = 0;
  const state = {
    host,
    objects: [],
    parts: [],
    activeId: null,
    supported: true,
    desktop: true,
    mutations: [],
    batches: [],
    hooks: [],
    beforeHooks: [],
    failures: [],
    containerId: "container-1",
  };
  const nextId = () => (host === "Word" ? ++serial : `shape-${++serial}`);
  const nullObject = { isNullObject: true };
  const live = (item) => item && !item.deleted;
  const shapeDefaults = () => ({
    id: nextId(),
    name: `Image ${serial}`,
    type: host === "PowerPoint" ? "GeometricShape" : "Image",
    level: 0,
    left: 0,
    top: 0,
    width: 72,
    height: 36,
    rotation: 0,
    lockAspectRatio: false,
    placement: "Absolute",
    visible: true,
    altTextTitle: "",
    altTextDescription: "",
    fill: { type: "PictureAndTexture" },
    tags: [],
    parts: [],
    image: {
      cropLeft: 0,
      cropTop: 0,
      cropRight: 0,
      cropBottom: 0,
      brightness: 0.5,
      contrast: 0.5,
      colorType: "Automatic",
    },
    lineFormat: { visible: false },
  });
  const controlDefaults = (picture) => ({
    id: nextId(),
    tag: "",
    title: "",
    type: "RichText",
    subtype: "RichTextInline",
    cannotEdit: false,
    text: "\uFFFC",
    pictures: [picture],
    children: [],
    ooxml: '<w:drawing><a:xfrm rot="0"/><a:srcRect/></w:drawing>',
  });

  function runtimeContext() {
    let queue = [];
    const cache = new WeakMap();
    const enqueue = (label, fn, mutation = false) => queue.push({ label, fn, mutation });
    const context = {
      async sync() {
        const operations = queue;
        queue = [];
        const labels = operations.map((operation) => operation.label);
        state.batches.push(labels);
        // Matching hooks remove themselves; iterate a snapshot of the queue.
        const beforeHooks = [...state.beforeHooks];
        for (const hook of beforeHooks) {
          if (hook.test(labels)) {
            state.beforeHooks.splice(state.beforeHooks.indexOf(hook), 1);
            hook.run();
          }
        }
        for (const operation of operations) {
          operation.fn();
          if (operation.mutation) state.mutations.push(operation.label);
          const failure = state.failures.findIndex((item) => item.label === operation.label);
          if (failure !== -1) {
            const [{ error, onFail }] = state.failures.splice(failure, 1);
            onFail?.();
            throw error;
          }
        }
        const afterHooks = [...state.hooks];
        for (const hook of afterHooks) {
          if (hook.test(labels)) {
            state.hooks.splice(state.hooks.indexOf(hook), 1);
            hook.run();
          }
        }
      },
    };
    function proxy(data, kind, extras = {}) {
      if (cache.has(data)) return cache.get(data);
      const loaded = { isNullObject: !!data.isNullObject };
      const target = {
        ...extras,
        load(properties) {
          enqueue(`${kind}.load`, () => {
            loaded.isNullObject = !!data.isNullObject || !!data.deleted;
            if (!loaded.isNullObject)
              for (const key of properties.split(",")) {
                if (!(key in data) && key !== "zOrderPosition")
                  throw new Error(`Unknown ${kind} property ${key}`);
                loaded[key] =
                  key === "zOrderPosition" ? state.objects.filter(live).indexOf(data) : data[key];
              }
          });
          return result;
        },
      };
      const result = new Proxy(target, {
        get(object, key) {
          if (key in object) return object[key];
          if (key in loaded) return loaded[key];
          if (key === "then") return undefined;
          throw new Error(`PropertyNotLoaded: ${kind}.${String(key)}`);
        },
        set(object, key, value) {
          if (key === "placement" && !["Absolute", "OneCell", "TwoCell"].includes(value))
            throw new Error(`Invalid placement ${value}`);
          if (key === "name")
            enqueue(
              `${kind}.${key}`,
              () => {
                if (
                  state.objects.some(
                    (other) => live(other) && other !== data && other.name === value,
                  )
                )
                  throw new Error("Duplicate shape name");
                data[key] = value;
              },
              true,
            );
          else
            enqueue(
              `${kind}.${String(key)}`,
              () => {
                data[key] = value;
              },
              true,
            );
          return true;
        },
      });
      cache.set(data, result);
      return result;
    }
    function collection(getItems, wrap, label) {
      let items;
      return {
        get items() {
          if (!items) throw new Error(`PropertyNotLoaded: ${label}.items`);
          return items;
        },
        load(properties) {
          enqueue(`${label}.load`, () => {
            items = getItems().filter(live).map(wrap);
            const fields = properties
              .split(",")
              .map((field) => field.replace(/^items\//, ""))
              .filter((field) => field !== "items");
            // Child loads are fulfilled in the same host response.
            for (const item of items) if (fields.length) item.load(fields.join(","));
            const nested = queue;
            queue = [];
            for (const operation of nested) operation.fn();
          });
          return this;
        },
      };
    }
    function xmlParts(parts) {
      const wrap = (data) =>
        proxy(data, "part", {
          getXml() {
            const result = {};
            enqueue("part.getXml", () => {
              result.value = data.xml;
            });
            return result;
          },
          setXml(xml) {
            enqueue(
              "part.setXml",
              () => {
                data.xml = xml;
              },
              true,
            );
          },
        });
      return {
        getByNamespace(namespace) {
          return collection(
            () => parts.filter((part) => part.xml.includes(namespace)),
            wrap,
            "parts",
          );
        },
        add(xml) {
          const data = { id: `part-${++serial}`, xml };
          enqueue("part.add", () => parts.push(data), true);
          return wrap(data);
        },
      };
    }
    function picture(data, _control) {
      return proxy(data, "picture", {
        insertContentControl() {
          const added = controlDefaults(data);
          enqueue(
            "word.insertControl",
            () => {
              state.objects.push(added);
              state.activeId = added.id;
            },
            true,
          );
          return controlProxy(added);
        },
      });
    }
    function newPicture(png) {
      return {
        png,
        width: 72,
        height: 36,
        lockAspectRatio: false,
        altTextTitle: "",
        altTextDescription: "",
        hyperlink: "",
      };
    }
    function controlProxy(data) {
      return proxy(data, "control", {
        inlinePictures: collection(
          () => data.pictures || [],
          (item) => picture(item, data),
          "pictures",
        ),
        contentControls: collection(() => data.children || [], controlProxy, "controls"),
        getOoxml() {
          const result = {};
          enqueue("word.getOoxml", () => {
            result.value = data.ooxml;
          });
          return result;
        },
        insertInlinePictureFromBase64(png, location) {
          if (location !== "Replace") throw new Error("Expected Replace");
          const added = newPicture(png);
          enqueue(
            "word.replace",
            () => {
              data.pictures = [added];
            },
            true,
          );
          return picture(added, data);
        },
        delete(keepContent) {
          if (keepContent !== false) throw new Error("KeepContent would leave preview behind");
          enqueue(
            "word.delete",
            () => {
              data.deleted = true;
            },
            true,
          );
        },
      });
    }
    function shapeProxy(data) {
      const tags = collection(
        () => data.tags || [],
        (item) => proxy(item, "tag"),
        "tags",
      );
      tags.add = (key, value) =>
        enqueue(
          "tag.add",
          () => {
            data.tags = data.tags.filter((tag) => tag.key !== key);
            data.tags.push({ key: key.toUpperCase(), value });
          },
          true,
        );
      return proxy(data, "shape", {
        tags,
        customXmlParts: xmlParts(data.parts || []),
        image: proxy(data.image || {}, "image"),
        lineFormat: proxy(data.lineFormat || {}, "lineFormat"),
        group: { shapes: collection(() => data.children || [], shapeProxy, "group.shapes") },
        line: proxy(data.line || {}, "line", {
          beginConnectedShape: proxy(
            data.line?.beginConnectedShape || { ...nullObject },
            "endpoint",
          ),
          endConnectedShape: proxy(data.line?.endConnectedShape || { ...nullObject }, "endpoint"),
        }),
        fill: proxy(data.fill || {}, "fill", {
          setImage(png) {
            enqueue(
              "ppt.setImage",
              () => {
                data.png = png;
                data.fill.type = "PictureAndTexture";
              },
              true,
            );
          },
        }),
        delete() {
          enqueue(
            `${host}.delete`,
            () => {
              data.deleted = true;
            },
            true,
          );
        },
        setZOrder(direction) {
          if (!["SendToBack", "BringForward"].includes(direction))
            throw new Error("Invalid z order");
          enqueue(
            "shape.setZOrder",
            () => {
              const objects = state.objects.filter(live),
                old = objects.indexOf(data);
              objects.splice(old, 1);
              objects.splice(
                direction === "SendToBack" ? 0 : Math.min(old + 1, objects.length),
                0,
                data,
              );
              state.objects = [...objects, ...state.objects.filter((item) => !live(item))];
            },
            true,
          );
        },
      });
    }
    const shapes = Object.assign(
      collection(() => state.objects, shapeProxy, "shapes"),
      {
        getItemOrNullObject(id) {
          return shapeProxy(
            state.objects.find((item) => live(item) && item.id === id) || { ...nullObject },
          );
        },
        addImage(png) {
          const added = { ...shapeDefaults(), png };
          enqueue(
            "excel.addImage",
            () => {
              state.objects.push(added);
              state.activeId = added.id;
            },
            true,
          );
          return shapeProxy(added);
        },
        addGeometricShape(type, geometry) {
          if (type !== "Rectangle") throw new Error("Expected Rectangle");
          const added = { ...shapeDefaults(), ...geometry };
          enqueue(
            "ppt.addShape",
            () => {
              state.objects.push(added);
              state.activeId = added.id;
            },
            true,
          );
          return shapeProxy(added);
        },
      },
    );
    const container = { id: state.containerId };
    const containerProxy = proxy(container, "container", { shapes });
    const activeShapes = () =>
      state.objects.filter((item) => live(item) && item.id === state.activeId);
    if (host === "Word") {
      context.document = {
        customXmlParts: xmlParts(state.parts),
        contentControls: {
          getByIdOrNullObject(id) {
            return controlProxy(
              state.objects.find((item) => live(item) && item.id === id) || { ...nullObject },
            );
          },
        },
        getSelection() {
          return {
            contentControls: collection(() => [], controlProxy, "selection.controls"),
            parentContentControlOrNullObject: controlProxy(activeShapes()[0] || { ...nullObject }),
            insertInlinePictureFromBase64(png, location) {
              if (location !== "End") throw new Error("Insertion must preserve selected text");
              return picture(newPicture(png));
            },
          };
        },
      };
    } else if (host === "Excel") {
      context.workbook = {
        customXmlParts: xmlParts(state.parts),
        worksheets: {
          getItemOrNullObject(id) {
            return id === state.containerId
              ? containerProxy
              : proxy({ ...nullObject }, "container");
          },
          getActiveWorksheet() {
            return containerProxy;
          },
        },
        getActiveShapeOrNullObject() {
          return shapeProxy(activeShapes()[0] || { ...nullObject });
        },
      };
    } else {
      context.presentation = {
        slides: {
          getItemOrNullObject(id) {
            return id === state.containerId
              ? containerProxy
              : proxy({ ...nullObject }, "container");
          },
        },
        getSelectedSlides() {
          return collection(
            () => [container],
            () => containerProxy,
            "selectedSlides",
          );
        },
        getSelectedShapes() {
          return collection(activeShapes, shapeProxy, "selectedShapes");
        },
      };
    }
    return context;
  }
  const runtime = {
    async run(callback) {
      return callback(runtimeContext());
    },
  };
  const Office = {
    context: {
      host,
      requirements: {
        isSetSupported(name) {
          return state.supported && (name !== "ExcelApiDesktop" || state.desktop);
        },
      },
    },
  };
  return {
    state,
    dependencies: { Office, [host]: runtime },
    object(id = state.activeId) {
      return state.objects.find((item) => live(item) && item.id === id);
    },
    copy(id = state.activeId) {
      const source = state.objects.find((item) => live(item) && item.id === id);
      const copied = structuredClone(source);
      copied.id = nextId();
      if (host !== "Word") copied.name += ` copy ${serial}`;
      state.objects.push(copied);
      state.activeId = copied.id;
      return copied;
    },
    failAfter(label, onFail) {
      state.failures.push({
        label,
        error: new Error(`Injected sync failure after ${label}`),
        onFail,
      });
    },
    afterBatch(test, run) {
      state.hooks.push({ test, run });
    },
    beforeBatch(test, run) {
      state.beforeHooks.push({ test, run });
    },
  };
}
