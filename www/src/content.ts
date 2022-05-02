import { DisplayNameChallenge, State, Violation } from './json';

const BadgeVerified = `
    <span class="badge bg-success">verified</span>
`;

const BadgeVerifiedHalf = `
    <span class="badge bg-info">verified (1/2)</span>
`;

const BadgeUnverified = `
    <span class="badge bg-warning text-dark">unverified</span>
`;

export const BadgeValid = `
    <span class="badge bg-success">valid</span>
`;

const BadgeInvalid = `
    <span class="badge bg-danger">invalid</span>
`;

// Manages the content in the UI. Mostly called within the `ActionListener`.
export class ContentManager {
    btn_execute_action: HTMLButtonElement;
    div_live_updates_info: HTMLElement;
    div_display_name_overview: HTMLElement;
    div_fully_verified_info: HTMLElement;
    div_verification_overview: HTMLElement;
    div_email_second_challenge: HTMLElement;
    div_unsupported_overview: HTMLElement;

    constructor() {
        // Register relevant elements.
        this.btn_execute_action =
            document
                .getElementById("execute-action")! as HTMLButtonElement;

        this.div_live_updates_info =
            document
                .getElementById("div-live-updates-info")!;

        this.div_display_name_overview =
            document
                .getElementById("div-display-name-overview")!;

        this.div_fully_verified_info =
            document
                .getElementById("div-fully-verified-info")! as HTMLButtonElement;

        this.div_verification_overview =
            document
                .getElementById("div-verification-overview")!;

        this.div_email_second_challenge =
            document
                .getElementById("div-email-second-challenge")!;

        this.div_unsupported_overview =
            document
                .getElementById("div-unsupported-overview")!;
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
        document.getElementById("introduction")!.innerHTML = "";
    }
    processVerificationOverviewTable(state: State) {
        let table = "";

        let counter = 1;
        for (let field of state.fields) {
            if (field.challenge.type == "expected_message") {
                let validity;
                if (field.challenge.content.expected.is_verified) {
                    if (field.challenge.content.second && !field.challenge.content.second!.is_verified) {
                        validity = BadgeVerifiedHalf;

                        this.setEmailSecondChallengeContent(field.value.value);
                    } else {
                        validity = BadgeVerified;

                        if (field.value.type == "email") {
                            this.wipeEmailSecondChallengeContent();
                        }
                    }
                } else {
                    validity = BadgeUnverified;
                }

                // Specify the destination address.
                let to = "N/A";
                if (field.value.type == "email") {
                    to = "registrar@web3.foundation";
                } else if (field.value.type == "twitter") {
                    to = "@w3f_registrar";
                } else if (field.value.type == "matrix") {
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
            } else if (field.challenge.type == "display_name_check") {
                let validity;

                let challenge: DisplayNameChallenge = field.challenge.content;
                if (challenge.passed) {
                    this.setDisplayNameVerification(field.value.value, BadgeValid);
                } else {
                    validity = BadgeInvalid;
                    this.setDisplayNameViolation(field.value.value, challenge.violations, true);
                }
            }
        }

        // Apply table to the page.
        this.setVerificationOverviewContent(table);
    }
    processUnsupportedOverview(state: State) {
        let unsupported = "";
        for (let field of state.fields) {
            if (field.challenge.type == "unsupported") {
                unsupported += `<li>${capitalizeFirstLetter(field.value.type)} ("${field.value.value}")</li>`;
            }
        }

        if (unsupported.length != 0) {
            this.setUnsupportedContent(unsupported);
        } else {
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
    setDisplayNameVerification(name: string, validity: string) {
        this.div_display_name_overview.innerHTML = `
            <div class="col-10 ">
                <h2>Display name check</h2>
                <p>The display name <strong>${name}</strong> is ${validity}</p>
            </div>
        `;
    }
    setDisplayNameViolation(name: string, violations: Violation[], show_hint: boolean) {
        let listed = "";
        for (let v of violations) {
            listed += `<li>"${v.display_name}" (by account <em>${v.context.address}</em>)</li>`
        }

        let hint = "";
        if (show_hint) {
            hint = `<p><strong>Hint:</strong> You can check for valid display names by selecting <em>"Validate Display Name"</em> in the search bar.</p>`
        }

        this.div_display_name_overview.innerHTML = `
            <div class="col-10 ">
                <h2>Display name check</h2>
                <p>The display name <strong>${name}</strong> is ${BadgeInvalid}. It's too similar to (an) existing display name(s):</p>
                <ul>
                    ${listed}
                </ul>
                ${hint}
            </div>
        `;
    }
    setVerificationOverviewContent(table: string) {
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
    setEmailSecondChallengeContent(address: string) {
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
            .getElementById("specify-second-challenge")! as HTMLInputElement;

        let button = document
            .getElementById("execute-second-challenge")! as HTMLButtonElement;

        second_challenge
            .addEventListener("input", (_: Event) => {
                button.innerHTML = `Go!`;
                button.disabled = false;
            });

        button
            .addEventListener("click", async (e: Event) => {
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

                console.log(body);

                let response = await fetch("https://registrar-backend.web3.foundation/api/verify_second_challenge",
                    {
                        method: "POST",
                        headers: {
                            "Content-Type": "application/json",
                        },
                        body: body,
                    });

                // TODO:
                //let x = response.json();
            });
    }
    wipeEmailSecondChallengeContent() {
        this.div_email_second_challenge.innerHTML = "";
    }
    setUnsupportedContent(list: string) {
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

export function capitalizeFirstLetter(word: string) {
    return (word.charAt(0).toUpperCase() + word.slice(1))
        .replace("_", " ");
}
