import http from "node:http";

const port = Number(process.env.PORT || 19080);
const directories = new Set(["/quicknote"]);
const objects = new Map();

function normalizedPath(url) {
  return decodeURIComponent(new URL(url, "http://localhost").pathname).replace(/\/$/, "") || "/";
}

function immediateChildren(parent) {
  const prefix = `${parent}/`;
  const children = new Set();
  for (const path of [...directories, ...objects.keys()]) {
    if (!path.startsWith(prefix)) continue;
    const child = path.slice(prefix.length).split("/")[0];
    if (child) children.add(`${prefix}${child}`);
  }
  return [...children].sort();
}

const server = http.createServer((request, response) => {
  const path = normalizedPath(request.url);
  if (request.method === "MKCOL") {
    if (directories.has(path)) {
      response.writeHead(405).end();
      return;
    }
    const parent = path.slice(0, path.lastIndexOf("/")) || "/";
    if (!directories.has(parent)) {
      response.writeHead(409).end();
      return;
    }
    directories.add(path);
    response.writeHead(201).end();
    return;
  }

  if (request.method === "PROPFIND") {
    if (!directories.has(path)) {
      response.writeHead(404).end();
      return;
    }
    const hrefs = [path, ...immediateChildren(path)]
      .map((href) => `<d:response><d:href>${encodeURI(href)}/</d:href></d:response>`)
      .join("");
    response.writeHead(207, { "Content-Type": "application/xml" });
    response.end(`<d:multistatus xmlns:d="DAV:">${hrefs}</d:multistatus>`);
    return;
  }

  if (request.method === "PUT") {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      const body = Buffer.concat(chunks);
      if (request.headers["if-none-match"] === "*" && objects.has(path)) {
        response.writeHead(412).end();
        return;
      }
      objects.set(path, body);
      response.writeHead(201).end();
    });
    return;
  }

  if (request.method === "GET") {
    const body = objects.get(path);
    if (!body) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "Content-Type": "application/octet-stream" });
    response.end(body);
    return;
  }

  response.writeHead(405).end();
});

server.listen(port, "127.0.0.1", () => {
  process.stdout.write(`WebDAV fixture listening on ${port}\n`);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
