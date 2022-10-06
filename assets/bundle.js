(function(){function r(e,n,t){function o(i,f){if(!n[i]){if(!e[i]){var c="function"==typeof require&&require;if(!f&&c)return c(i,!0);if(u)return u(i,!0);var a=new Error("Cannot find module '"+i+"'");throw a.code="MODULE_NOT_FOUND",a}var p=n[i]={exports:{}};e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports,r,e,n,t)}return n[i].exports}for(var u="function"==typeof require&&require,i=0;i<t.length;i++)o(t[i]);return o}return r})()({1:[function(require,module,exports){
"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.capitalizeFirstLetter = exports.ContentManager = exports.BadgeValid = void 0;
const BadgeVerified = `
    <span class="badge bg-success">verified</span>
`;
const BadgeVerifiedHalf = `
    <span class="badge bg-info">verified (1/2)</span>
`;
const BadgeUnverified = `
    <span class="badge bg-warning text-dark">unverified</span>
`;
exports.BadgeValid = `
    <span class="badge bg-success">valid</span>
`;
const BadgeInvalid = `
    <span class="badge bg-danger">invalid</span>
`;
// Manages the content in the UI. Mostly called within the `ActionListener`.
class ContentManager {
    constructor(handler) {
        // Register relevant elements.
        this.btn_execute_action =
            document
                .getElementById("execute-action");
        this.div_live_updates_info =
            document
                .getElementById("div-live-updates-info");
        this.div_display_name_overview =
            document
                .getElementById("div-display-name-overview");
        this.div_fully_verified_info =
            document
                .getElementById("div-fully-verified-info");
        this.div_verification_overview =
            document
                .getElementById("div-verification-overview");
        this.div_email_second_challenge =
            document
                .getElementById("div-email-second-challenge");
        this.div_unsupported_overview =
            document
                .getElementById("div-unsupported-overview");
        this.notifications = handler;
    }
    setButtonLoadingSpinner() {
        this.btn_execute_action.disabled = true;
        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;
    }
    setButtonLiveAnimation() {
        this.btn_execute_action.innerHTML = `
            <div class="spinner-grow spinner-grow-sm" role="status">
                <span class="visually-hidden"></span>
            </div>
        `;
    }
    resetButton() {
        this.btn_execute_action.innerHTML = `Go!`;
        this.btn_execute_action.disabled = false;
    }
    wipeIntroduction() {
        document.getElementById("introduction").innerHTML = "";
    }
    processVerificationOverviewTable(state) {
        let table = "";
        let counter = 1;
        for (let field of state.fields) {
            if (field.challenge.type == "expected_message") {
                let validity;
                if (field.challenge.content.expected.is_verified) {
                    if (field.challenge.content.second && !field.challenge.content.second.is_verified) {
                        validity = BadgeVerifiedHalf;
                        this.setEmailSecondChallengeContent(field.value.value);
                    }
                    else {
                        validity = BadgeVerified;
                        if (field.value.type == "email") {
                            this.wipeEmailSecondChallengeContent();
                        }
                    }
                }
                else {
                    validity = BadgeUnverified;
                }
                // Specify the destination address.
                let to = "N/A";
                if (field.value.type == "email") {
                    to = "registrar@web3.foundation";
                }
                else if (field.value.type == "twitter") {
                    to = "@w3f_registrar";
                }
                else if (field.value.type == "matrix") {
                    to = "@registrar-v2:web3.foundation";
                }
                table += `
                        <tr>
                            <th scope="row">${counter}</th>
                            <td>${capitalizeFirstLetter(field.value.type)}</td>
                            <td>${field.challenge.content.expected.value}</td>
                            <td>${field.value.value}</td>
                            <td>${to}</td>
                            <td>${validity}</td>
                        </tr>
                    `;
                counter += 1;
            }
            else if (field.challenge.type == "display_name_check") {
                let validity;
                let challenge = field.challenge.content;
                if (challenge.passed) {
                    this.setDisplayNameVerification(field.value.value, exports.BadgeValid);
                }
                else {
                    validity = BadgeInvalid;
                    this.setDisplayNameViolation(field.value.value, challenge.violations, true);
                }
            }
        }
        // Apply table to the page.
        this.setVerificationOverviewContent(table);
    }
    processUnsupportedOverview(state) {
        let unsupported = "";
        for (let field of state.fields) {
            if (field.challenge.type == "unsupported") {
                unsupported += `<li>${capitalizeFirstLetter(field.value.type)} ("${field.value.value}")</li>`;
            }
        }
        if (unsupported.length != 0) {
            this.setUnsupportedContent(unsupported);
        }
        else {
            this.wipeUnsupportedContent;
        }
    }
    setLiveUpdateInfo() {
        this.div_live_updates_info.innerHTML = `
            <div class="col-10">
                <p class="text-center"><em>Displaying live updates...</em></p>
            </div>
        `;
    }
    wipeLiveUpdateInfo() {
        this.div_live_updates_info.innerHTML = "";
    }
    setDisplayNameVerification(name, validity) {
        this.div_display_name_overview.innerHTML = `
            <div class="col-10 ">
                <h2>Display name check</h2>
                <p>The display name <strong id="display-name-strong"></strong> is ${validity}</p>
            </div>
        `;
        document.getElementById("display-name-strong").innerText = name;
    }
    setDisplayNameViolation(name, violations, show_hint) {
        let listed = "";
        for (let v of violations) {
            listed += `<li>"${v.display_name}" (by account <em>${v.context.address}</em>)</li>`;
        }
        let hint = "";
        if (show_hint) {
            hint = `<p><strong>Hint:</strong> You can check for valid display names by selecting <em>"Validate Display Name"</em> in the search bar.</p>`;
        }
        this.div_display_name_overview.innerHTML = `
            <div class="col-10 ">
                <h2>Display name check</h2>
                <p>The display name <strong id="display-name-strong"></strong> is ${BadgeInvalid}. It's too similar to (an) existing display name(s):</p>
                <ul>
                    ${listed}
                </ul>
                ${hint}
            </div>
        `;
        document.getElementById("display-name-strong").textContent = name;
    }
    setVerificationOverviewContent(table) {
        this.div_verification_overview.innerHTML = `
            <div class="col-10 table-responsive ">
                <h2>Account verification</h2>
                <p>Send each provided challenge <strong>from</strong> your account <strong>to</strong> the corresponding W3F account.
                    You can just copy and paste the challenge directly.</p>
                <p><em>Note:</em> Twitter verification can take about 5 minutes.</p>
                <table id="verification-overview" class="table table-striped table-dark">
                    <thead>
                        <tr>
                            <th scope="col">#</th>
                            <th scope="col">Type</th>
                            <th scope="col">Challenge</th>
                            <th scope="col">From</th>
                            <th scope="col">To</th>
                            <th scope="col">Status</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${table}
                    </tbody>
                </table>
            </div>
        `;
    }
    wipeVerificationOverviewContent() {
        this.div_verification_overview.innerHTML = "";
    }
    setEmailSecondChallengeContent(address) {
        this.div_email_second_challenge.innerHTML = `
            <div class="col-10">
                <h2>⚠️️ Additional Challenge</h2>
                <p>A message was sent from <em>registrar@web3.foundation</em> to <strong>${address}</strong> containing an additional challenge
                    (make sure to check the spam folder). Please insert that challenge into the following field:
                </p>
                <div class="input-group">
                <input id="specify-second-challenge" type="text" class="form-control"
                    aria-label="Second challenge verification" placeholder="Challenge...">
                <button id="execute-second-challenge" class="col-1 btn btn-primary"
                    type="button">Verify</button>
                </div>
            </div>`;
        let second_challenge = document
            .getElementById("specify-second-challenge");
        let button = document
            .getElementById("execute-second-challenge");
        second_challenge
            .addEventListener("input", (_) => {
            button.innerHTML = `Go!`;
            button.disabled = false;
        });
        button
            .addEventListener("click", (e) => __awaiter(this, void 0, void 0, function* () {
            button.disabled = true;
            button
                .innerHTML = `
                        <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                        <span class="visually-hidden"></span>
                    `;
            let body = JSON.stringify({
                entry: {
                    type: "email",
                    value: address,
                },
                challenge: second_challenge.value,
            });
            let _resp = yield fetch("https://registrar-backend.web3.foundation/api/verify_second_challenge", {
                method: "POST",
                headers: {
                    "Content-Type": "application/json",
                },
                body: body,
            });
            // No need to check the result, since an appropriate event is
            // generated in the backend and submitted over the websocket
            // stream.
            // Reset elements.
            button.disabled = false;
            button.innerHTML = "Verify";
            second_challenge.value = "Challenge...";
        }));
    }
    wipeEmailSecondChallengeContent() {
        this.div_email_second_challenge.innerHTML = "";
    }
    setUnsupportedContent(list) {
        this.div_unsupported_overview.innerHTML = `
            <div class="col-10">
                <h2>🚨 Unsupported entries</h2>
                <ul>
                    ${list}
                </ul>
                <p>The identity on-chain info contains fields that are not supported by the W3F registrar service in
                an automated manner and <em>must</em> be removed. If you really want to have those fields
                included, contact the appropriate authorities as described in the <em>"Need help?"</em> section below. Please prepare
                the necessary information so the manual verification can be completed as quickly as possible. For
                example, if you want to add a web address, make sure that the website somehow references
                your Kusama/Polkadot address.</p>
            </div>
        `;
    }
    wipeUnsupportedContent() {
        this.div_unsupported_overview.innerHTML = "";
    }
}
exports.ContentManager = ContentManager;
function capitalizeFirstLetter(word) {
    return (word.charAt(0).toUpperCase() + word.slice(1))
        .replace("_", " ");
}
exports.capitalizeFirstLetter = capitalizeFirstLetter;

},{}],2:[function(require,module,exports){
"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
Object.defineProperty(exports, "__esModule", { value: true });
const content_1 = require("./content");
const notifications_1 = require("./notifications");
const config = require("../config.json");
// The primary manager of all actions/events, for both UI and server messages.
class ActionListerner {
    constructor() {
        // Register relevant elements.
        this.btn_execute_action =
            document
                .getElementById("execute-action");
        this.specify_action =
            document
                .getElementById("specify-action");
        this.specify_network =
            document
                .getElementById("specify-network");
        this.search_bar =
            document
                .getElementById("search-bar");
        const handler = new notifications_1.NotificationHandler;
        this.manager = new content_1.ContentManager(handler);
        this.notifications = handler;
        // Handler for choosing network, e.g. "Kusama" or "Polkadot".
        document
            .getElementById("network-options")
            .addEventListener("click", (e) => {
            this.specify_network
                .innerText = e.target.innerText;
            this.manager.resetButton();
        });
        // Handler for choosing action, e.g. "Check Judgement".
        document
            .getElementById("action-options")
            .addEventListener("click", (e) => {
            let target = e.target.innerText;
            if (target == "Check Judgement") {
                this.search_bar.placeholder = "Account address...";
                this.specify_action.innerText = target;
            }
            else if (target == "Validate Display Name") {
                this.search_bar.placeholder = "Display Name...";
                this.specify_action.innerText = target;
            }
        });
        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_) => {
            let action = this.specify_action.innerHTML;
            if (action == "Check Judgement") {
                window.location.href = "?network="
                    + this.specify_network.innerHTML.toLowerCase()
                    + "&address="
                    + this.search_bar.value;
            }
            else if (action == "Validate Display Name") {
                this.executeAction();
            }
        });
        this.search_bar
            .addEventListener("input", (_) => {
            this.manager.resetButton();
            if (this.search_bar.value.startsWith("1")) {
                this.specify_network.innerHTML = "Polkadot";
            }
            else {
                this.specify_network.innerHTML = "Kusama";
            }
        });
        // Bind 'Enter' key to action button.
        this.search_bar
            .addEventListener("keyup", (event) => {
            // Number 13 is the "Enter" key on the keyboard
            if (event.keyCode === 13) {
                // Cancel the default action, if needed
                event.preventDefault();
                this.btn_execute_action.click();
            }
        });
        // Add a listener for every notification. Required for closing.
        Array.from(document
            .getElementsByClassName("toast"))
            .forEach(element => {
            element
                .addEventListener("click", (_) => {
            });
        });
        // Get params from the webbrowser search bar, load data from server if
        // specified.
        let params = new URLSearchParams(window.location.search);
        let network = params.get("network");
        let address = params.get("address");
        if (network != null && address != null) {
            this.specify_network.innerHTML = (0, content_1.capitalizeFirstLetter)(network);
            this.search_bar.value = address;
            this.executeAction();
        }
    }
    // Executes the main logic, either the judgement state or display name check.
    executeAction() {
        this.manager.setButtonLoadingSpinner();
        const action = this.specify_action.innerHTML;
        const user_input = this.search_bar.value;
        const network = this.specify_network.innerHTML.toLowerCase();
        if (action == "Check Judgement") {
            const socket = new WebSocket(config.ws_url);
            window.setInterval(() => {
                socket.send("heartbeat");
            }, 30000);
            // Send request to the server
            socket.onopen = () => {
                let msg = JSON.stringify({ address: user_input, chain: network });
                socket.send(msg);
            };
            // Parse received judgement state.
            socket.onmessage = (event) => {
                let msg = event;
                this.handleJudgementState(msg);
            };
        }
        else if (action == "Validate Display Name") {
            let display_name = user_input;
            (() => __awaiter(this, void 0, void 0, function* () {
                let body = JSON.stringify({
                    check: display_name,
                    chain: network,
                });
                let response = yield fetch(config.http_url, {
                    method: "POST",
                    headers: {
                        "Content-Type": "application/json",
                    },
                    body: body,
                });
                let result = JSON.parse(yield response.text());
                this.handleDisplayNameCheck(result, display_name);
            }))();
        }
    }
    // Handles the display name result received from the server.
    handleDisplayNameCheck(data, display_name) {
        this.manager.wipeIntroduction();
        if (data.type == "ok") {
            let check = data.message;
            if (check.type == "ok") {
                this.manager.setDisplayNameVerification(display_name, content_1.BadgeValid);
            }
            else if (check.type = "violations") {
                let violations = check.value;
                this.manager.setDisplayNameViolation(display_name, violations, false);
            }
            else {
                // Should never occur.
                this.notifications.unexpectedError("pdnc#1");
            }
        }
        else if (data.type == "err") {
            // Should never occur.
            this.notifications.unexpectedError("pdnc#2");
        }
        else {
            // Should never occur.
            this.notifications.unexpectedError("pdnc#3");
        }
        this.manager.resetButton();
        this.manager.wipeLiveUpdateInfo();
        this.manager.wipeVerificationOverviewContent();
        this.manager.wipeEmailSecondChallengeContent();
        this.manager.wipeUnsupportedContent();
    }
    // Handles the judgement state received from the server.
    handleJudgementState(msg) {
        const parsed = JSON.parse(msg.data);
        if (parsed.type == "ok") {
            let message = parsed.message;
            this.manager.wipeIntroduction();
            this.manager.setButtonLiveAnimation();
            this.manager.setLiveUpdateInfo();
            this.manager.processVerificationOverviewTable(message.state);
            this.manager.processUnsupportedOverview(message.state);
            this.notifications.processNotifications(message.notifications);
            // This notification should only be displayed if no other notifications are available.
            if (message.state.is_fully_verified && message.notifications.length == 0) {
                this.notifications.displayNotification("The identity has been fully verified!", "bg-success text-light", true);
            }
        }
        else if (parsed.type == "err") {
            let message = parsed.message;
            this.notifications.displayError(message);
            this.manager.resetButton();
            this.manager.wipeLiveUpdateInfo();
        }
        else {
            // Should never occur.
            this.notifications.unexpectedError("pas#1");
        }
    }
}
new ActionListerner();

},{"../config.json":4,"./content":1,"./notifications":3}],3:[function(require,module,exports){
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.NotificationHandler = void 0;
const content_js_1 = require("./content.js");
class NotificationHandler {
    constructor() {
        this.notify_idx = 0;
        this.div_notifications =
            document
                .getElementById("div-notifications");
    }
    processNotifications(notifications) {
        for (let notify of notifications) {
            try {
                const [message, color] = notificationTypeResolver(notify);
                this.displayNotification(message, color, false);
            }
            catch (error) {
                this.unexpectedError("pnntr#1");
            }
        }
    }
    displayNotification(message, color, show_final) {
        this.div_notifications.insertAdjacentHTML("beforeend", `<div id="toast-${this.notify_idx}" class="toast show align-items-center ${color} border-0" role="alert" aria-live="assertive"
                    aria-atomic="true">
                    <div class="d-flex">
                        <div class="toast-body">
                            ${message}
                        </div>
                        <button id="toast-${this.notify_idx}-close-btn" type="button" class="btn-close btn-close-white me-2 m-auto" data-bs-dismiss="toast"
                            aria-label="Close"></button>
                    </div>
                </div>
            `);
        // Add handler for close button.
        let idx = this.notify_idx;
        document
            .getElementById(`toast-${idx}-close-btn`)
            .addEventListener("click", (e) => {
            let toast = document
                .getElementById(`toast-${idx}`);
            toast.classList.remove("show");
            toast.classList.add("hide");
        });
        // Cleanup old toast, limit to eight max.
        let max = 8;
        if (show_final) {
            max = 1;
        }
        let old = this.notify_idx - max;
        if (old >= 0) {
            let toast = document
                .getElementById(`toast-${old}`);
            if (toast) {
                toast.classList.remove("show");
                toast.classList.add("hide");
            }
        }
        this.notify_idx += 1;
    }
    displayError(message) {
        this.displayNotification(message, "bg-danger text-light", false);
    }
    unexpectedError(id) {
        this.displayError(`Unexpected internal error, please contact admin. Code: ${id}`);
    }
}
exports.NotificationHandler = NotificationHandler;
function notificationTypeResolver(notification) {
    switch (notification.type) {
        case "identity_inserted": {
            return [
                `The judgement request has been discovered by the registrar service.`,
                "bg-info text-dark"
            ];
        }
        case "identity_updated": {
            return [
                `On-chain identity information has been modified.`,
                "bg-info text-dark"
            ];
        }
        case "field_verified": {
            let data = notification.value;
            return [
                `${(0, content_js_1.capitalizeFirstLetter)(data.field.type)} account "${data.field.value}" is verified. Challenge is valid.`,
                "bg-success text-light",
            ];
        }
        case "field_verification_failed": {
            let data = notification.value;
            return [
                `${(0, content_js_1.capitalizeFirstLetter)(data.field.type)} account "${data.field.value}" failed to get verified. Invalid challenge.`,
                "bg-danger text-light"
            ];
        }
        case "second_field_verified": {
            let data = notification.value;
            return [
                `${(0, content_js_1.capitalizeFirstLetter)(data.field.type)} account "${data.field.value}" is fully verified. Additional challenge is valid.`,
                "bg-success text-light"
            ];
        }
        case "second_field_verification_failed": {
            let data = notification.value;
            return [
                `${(0, content_js_1.capitalizeFirstLetter)(data.field.type)} account "${data.field.value}" failed to get verified. The additional challenge is invalid.`,
                "bg-danger text-light"
            ];
        }
        case "awaiting_second_challenge": {
            let data = notification.value;
            return [
                `A second challenge was sent to ${(0, content_js_1.capitalizeFirstLetter)(data.field.type)} account "${data.field.value}". Please also check the spam folder.`,
                "bg-info text-dark"
            ];
        }
        case "identity_fully_verified": {
            return [
                `<strong>Verification process completed!</strong> Judgement will be issued in a couple of minutes.`,
                "bg-success text-light"
            ];
        }
        case "judgement_provided": {
            return [
                `Judgement has been submitted!`,
                "bg-success text-light"
            ];
        }
        case "manually_verified": {
            let data = notification.value;
            return [
                `Manually verified ${(0, content_js_1.capitalizeFirstLetter)(data.field)}`,
                "bg-info text-light"
            ];
        }
        case "full_manual_verification": {
            return [
                `Manually verified the identity. Judgement will be issued in a couple of minutes.`,
                "bg-info text-light"
            ];
        }
        default: {
            throw new Error("unrecognized notification");
        }
    }
}

},{"./content.js":1}],4:[function(require,module,exports){
module.exports={
	"http_url": "https://registrar-backend.web3.foundation/api/check_display_name",
	"ws_url": "wss://registrar-backend.web3.foundation/api/account_status"
}
},{}]},{},[2]);
