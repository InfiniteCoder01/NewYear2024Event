// --------------------------------------- APIs --------------------------------------- //
window.print = (text) => {
    $('#console').append(`<div>${text}</div>`)
};

window.error = (text) => {
    $('#console').append(`<pre style=\"color: red;\">${text}</pre>`)
};

let backgroundTasks = [];
window.repeat = function (callback, interval) {
    backgroundTasks.push(setInterval(callback, interval));
};

import processMessage from "/controller/api.js";

let socket;
window.registerClient = (callback) => {
    if (socket) socket.close();
    socket = new WebSocket(
        `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${document.location.host}/connect/tetro/${account.uid}`
    );

    socket.onmessage = msg => {
        processMessage(msg, callback);
    };
}

window.sendMessage = (message) => {
    socket.send(message);
}

window.vec2 = function (x, y) { return { x, y }; };

// --------------------------------------- Run --------------------------------------- //
setInterval(() => $('#console').scrollTop($('#console')[0].scrollHeight), 25);

const run = (code, language) => {
    for (let task of backgroundTasks) {
        clearInterval(task);
    }
    backgroundTasks = [];
    $('#console').empty();

    if (language == "ace/mode/javascript") {
        // eval(prelude + code);
    } else if (language == "ace/mode/python") {
        run_python(code);
    }

};

export default run;