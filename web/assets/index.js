// Size of chunks.
const CHUNK_SIZE = 4 * 1024 * 1024;
// Files of which the size is <= `ONESHOT_THRESHOLD` will be uploaded directly without chunking.
const ONESHOT_THRESHHOLD = CHUNK_SIZE;
// Number of workers, per which uploads one file at a time.
const CONCURRENT_WORKER = 3;

function size_to_readable(size) {
    if (size === 0) {
        return "0 B";
    }
    const log = (base, number) => Math.log(number) / Math.log(base);
    const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    const level = Math.min(Math.floor(log(1024, size)), units.length - 1);
    const number = size / 1024 ** level;
    return `${number.toFixed(2)} ${units[level]}`;
}
function sleep(period) {
    return new Promise((resolve, _reject) => setTimeout(resolve, period));
}
// TODO: No support for IE
class Task {
    constructor(id, file) {
        this.id = id;
        this.file = file;
        this.progress_fn = null;
    }

    setProgress(progress) {
        this.progress_fn && this.progress_fn(progress)
    }

    setProgressFn(progress_fn) {
        this.progress_fn = progress_fn;
    }
}

const TASKS = { pending: new Array(), failed: new Map() };
const WORKERS = new Map();

const dropZone = document.getElementById("drop-zone");
const fileInput = document.getElementById("file-input");
const taskList = document.getElementById("task-list");
const fileNameList = document.getElementById("file-name-list");
const uploadButton = document.getElementById("upload-button");
const taskItemTemplate = document.getElementById("task-item-template");
// TODO: in EDGE: SCRIPT438: Object doesn't support property or method 'fromEntries'
const statusTemplate = Object.fromEntries(
    ["pending", "progress", "failed", "done"].map(
        status => [status, document.getElementById(`status-${status}-template`)]));
let filesSelected = null;
let taskId = 0;
let workerToken = 0;

dropZone.addEventListener("drop", function (event) {
    console.log("Dropped.");
    //const files = (new Array(event.dataTransfer.items)).filter(item => item.kind === "file").map(item => item.getAsFile());
    const files = event.dataTransfer.files;
    if (files.length === 0) {
        // alert("Only files are supported.");
        return;
    }
    event.preventDefault();
    onSelectFiles(files);
});
dropZone.addEventListener("dragover", function (event) {
    console.log("Dragging over.");
    event.preventDefault();
});
fileInput.addEventListener("change", () => onSelectFiles(fileInput.files));
uploadButton.addEventListener("click", onUpload);

function onSelectFiles(files) {
    // TODO: handle zero properly
    if (files.length === 0) {
        fileNameList.textContent = `No files selected.`;
        uploadButton.setAttribute("disabled", true);
    }
    else {
        const file_names = Array.from(files).map((file) => file.name).join(", ");
        fileNameList.innerHTML = `<small>${files.length} selected:</small> ${file_names} .`;
        uploadButton.removeAttribute("disabled");
        fileNameList.title = file_names;
        console.log(`File selected ${files.length}`);
        console.log(files);
    }
    filesSelected = files;
}

async function onUpload() {
    const files = filesSelected;
    for (const file of files) {
        taskId += 1;
        const taskItem = document.importNode(taskItemTemplate.content, true);
        taskItem.firstElementChild.setAttribute("id", `task-item-${taskId}`);
        taskItem.firstElementChild.dataset.id = taskId;
        console.log(file);
        const statusPending = document.importNode(statusTemplate.pending.content, true);
        taskItem.querySelector("[name=id]").textContent = taskId;
        taskItem.querySelector("[name=name]").textContent = file.name;
        taskItem.querySelector("[name=size]").textContent = size_to_readable(file.size);
        taskItem.querySelector("[name=status]").appendChild(statusPending);
        taskList.insertBefore(taskItem, taskList.firstElementChild);
        TASKS.pending.push(new Task(taskId, file));
    }
    // Clean selected files
    filesSelected = null;
    fileInput.value = "";
    fileInput.dispatchEvent(new Event("change"));
/*
    const files = document.getElementById("file-input").files;
    for (const file of files) {
        const start_at = Date.now();
        const metadata = {
            'file_name': file.name,
            'file_size': file.size,
            'chunk_size': CHUNK_SIZE
        };
        console.log("Uploading", metadata);
        const task = await (await fetch("/upload/start", {
            method: "POST",
            headers: { 'Content-Type': "application/json" },
            body: JSON.stringify(metadata)
        })).json();
        if (task.ok) {
            const chunk_number = Math.ceil(file.size / CHUNK_SIZE);
            const file_token = task.file_token;
            let broken = false;
            for (let chunk_index = 0; chunk_index < chunk_number; chunk_index++) {
                const chunk = await (await fetch(`/upload/${file_token}/${chunk_index}`, {
                    method: "POST",
                    headers: { 'Content-Type': "application/octet-stream" },
                    body: file.slice(chunk_index * CHUNK_SIZE, (chunk_index + 1) * CHUNK_SIZE)
                })).json();
                if (chunk.ok) {
                    console.log(`Uploaded ${chunk_index + 1}/${chunk_number} chunks.`);
                }
                else {
                    console.error(chunk.error);
                    broken = true;
                    break;
                }
            }
            if (!broken) {
                const result = await (await fetch("/upload/finish", {
                    'method': "POST",
                    headers: { 'Content-Type': "application/json" },
                    body: JSON.stringify({
                        file_token: file_token
                    })
                })).json();
                if (result.ok) {
                    const elapsed = Date.now() - start_at;
                    console.log(`Successfully uploaded ${file.name} with \
                            ${file.size / 1024 / 1024} MiBs in ${elapsed / 1000} seconds at \
                            ${file.size / 1024 / 1024 / (elapsed / 1000)} MiB/sec.`);
                }
            }
            else {
                console.error(`Failed to upload ${file.name} due to previous error.`);
            }
        }
        else {
            console.error(task.error);
        }
    }*/
}

async function upload_worker(token) {
    console.log(`Worker [${token}] started.`);
    while (WORKERS.get(token) !== undefined) {
        if (TASKS.pending.length === 0) {
            await sleep(100);
        }
        else {
            const task = TASKS.pending.shift();
            console.log(`Worker [${token}] fetched task ${task.id} with a file named ${task.file.name}`);
            const statusProgress = document.importNode(statusTemplate.progress.content, true);
            const taskItem = document.getElementById(`task-item-${task.id}`);
            const oldStatus = taskItem.querySelector("[name=status]")
            oldStatus.replaceChild(statusProgress, oldStatus.firstElementChild);
            const progress = statusProgress.querySelector("[name=progress]");
            const progress_fn = function(progress) {
                progress_fn.textContent = `${progress} %`;
                progress_fn.setAttribute("value", progress);
            };
            task.setProgressFn(progress_fn);
            // try
            await upload(task);
        }
    }
    console.log(`Worker [${token}] ends.`);
}

async function upload(task) {
    const file = task.file;
    const start_at = Date.now();
    const metadata = {
        'file_name': file.name,
        'file_size': file.size,
        'chunk_size': CHUNK_SIZE
    };
    console.log("Uploading", metadata);
    // Here treat `job` as initialized uploading instance of `task`.
    const job = await (await fetch("/upload/start", {
        method: "POST",
        headers: { 'Content-Type': "application/json" },
        body: JSON.stringify(metadata)
    })).json();
    if (job.ok) {
        const chunk_number = Math.ceil(file.size / CHUNK_SIZE);
        const file_token = job.file_token;
        let broken = false;
        for (let chunk_index = 0; chunk_index < chunk_number; chunk_index++) {
            const chunk = await (await fetch(`/upload/${file_token}/${chunk_index}`, {
                method: "POST",
                headers: { 'Content-Type': "application/octet-stream" },
                body: file.slice(chunk_index * CHUNK_SIZE, (chunk_index + 1) * CHUNK_SIZE)
            })).json();
            if (chunk.ok) {
                task.setProgress((chunk_index + 1) / (chunk_number));
                console.log(`Uploaded ${chunk_index + 1}/${chunk_number} chunks.`);
            }
            else {
                throw Exception("TODO");
                console.error(chunk.error);
                broken = true;
                break;
            }
        }
        if (!broken) {
            const result = await (await fetch("/upload/finish", {
                'method': "POST",
                headers: { 'Content-Type': "application/json" },
                body: JSON.stringify({
                    file_token: file_token
                })
            })).json();
            if (result.ok) {
                const elapsed = Date.now() - start_at;
                console.log(`Successfully uploaded ${file.name} with \
                        ${file.size / 1024 / 1024} MiBs in ${elapsed / 1000} seconds at \
                        ${file.size / 1024 / 1024 / (elapsed / 1000)} MiB/sec.`);
            }
        }
        else {
            console.error(`Failed to upload ${file.name} due to previous error.`);
        }
    }
    else {
        throw Exception("TODO");
        console.error(task.error);
    }
    const elapsed = Date.now() - start_at;
    return elapsed;
}

fileInput.dispatchEvent(new Event("change"));
for (let i = 0; i < CONCURRENT_WORKER; i++) {
    WORKERS.set(workerToken, null);
    WORKERS.set(workerToken, upload_worker(workerToken));
    workerToken += 1;
}