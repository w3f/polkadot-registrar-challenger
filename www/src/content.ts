import { State } from './json';

export class ContentManager {
	div_display_name_overview: HTMLElement;
	display_name_value: HTMLElement;
	display_name_validity: HTMLElement;
    div_fully_verified_info: HTMLElement;

	div_verification_overview: HTMLElement;
	verification_overview: HTMLElement;

	div_email_second_challenge: HTMLElement;
	email_second_challenge: HTMLElement;

	div_unsupported_overview: HTMLElement;
	unsupported_overview: HTMLElement;

	constructor() {
		this.div_display_name_overview =
			document
				.getElementById("div-display-name-overview")!;

		this.display_name_value =
			document
				.getElementById("display-name-value")!;

		this.display_name_validity =
			document
				.getElementById("display-name-validity")!;

        this.div_fully_verified_info =
            document
                .getElementById("div-fully-verified-info")! as HTMLButtonElement;

		this.div_verification_overview =
			document
				.getElementById("div-verification-overview")!;

		this.verification_overview =
			document
				.getElementById("verification-overview")!;

		this.div_email_second_challenge =
			document
				.getElementById("div-email-second-challenge")!;

		this.email_second_challenge =
			document
				.getElementById("email-second-challenge")!;

		this.div_unsupported_overview =
			document
				.getElementById("div-unsupported-overview")!;

		this.unsupported_overview =
			document
				.getElementById("unsupported-overview")!;
	}

	processVerificationOverviewTable(state: State) {
        // TODO: Check if 'fields` is empty.

        let table = `
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
        `;

        let counter = 1;
		for (let field of state.fields) {
			if (field.challenge.challenge_type == "expected_message") {
				let validity;
				if (field.challenge.content.expected.is_verified) {
					if (field.challenge.content.second && !field.challenge.content.second!.is_verified) {
						validity = BadgeVerifiedHalf;

						this.email_second_challenge.innerHTML = `${field.value.value}`;
					} else {
						validity = BadgeVerified;

                        /*
						if (field.value.type == "email") {
							this.div_email_second_challenge.classList.add("invisible");
						}
                        */
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
					to = "@registrar:web3.foundation";
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
			} else if (field.challenge.challenge_type == "background_check" && field.value.type == "display_name") {
				let elem = this.display_name_validity;

				if (field.challenge.content.passed) {
                    elem.innerHTML = BadgeValid;
				} else {
					elem.innerHTML = BadgeInvalid;
				}

				this.display_name_value.innerHTML = field.value.value;
			}

            table += '</tbody>';

            // Apply table to the page.
            this.verification_overview.innerHTML = table;

            // Display banner if fully verified.
            if (state.is_fully_verified) {
                this.div_fully_verified_info.innerHTML = ContentFullyVerified;
            }
		}
	}
}

export function capitalizeFirstLetter(word: string) {
    return (word.charAt(0).toUpperCase() + word.slice(1))
        .replace("_", " ");
}

const BadgeVerified = `
    <span class="badge bg-success">verified</span>
`;

const BadgeVerifiedHalf = `
    <span class="badge bg-info">verified (1/2)</span>
`;

const BadgeUnverified = `
    <span class="badge bg-warning text-dark">unverified</span>
`;

const BadgeValid = `
    <span class="badge bg-success">valid</span>
`;

const BadgeInvalid = `
    <span class="badge bg-danger">invalid</span>
`;

const ContentLiveUpdates = `
	<div class="col-10">
		<p class="text-center"><em>Displaying live updates...</em></p>
	</div>
`;

const ContentFullyVerified = `
	<div class="row justify-content-center">
		<div class="col-10 table-responsive bg-success p-2">
			<h2 class="text-center text-white">Identity fully verified! ✔</h2>
		</div>
	</div>
`;

const ContentDisplayNameResult = `
	<div class="col-10 ">
		<h2>Display name check</h2>
		<p>The display name <strong id="display-name-value"></strong> is <span
			id="display-name-validity"></span></p>
	</div>
`;

const ContentUnsupportedWarnings = `
	<div class="col-10">
		<h2>🚨 Unsupported entries</h2>
		<ul id="unsupported-overview">
		</ul>
		<p>The identity on-chain info contains fields that are not supported by the W3F registrar service in
		an automated manner and <em>must</em> be removed. If you really want to have those fields
		included, please contact the
		appropriate authorities as described in the <em>"Need help?"</em> section below. Please prepare
		the
		appropriate information so the manual verification can be completed as quickly as possible. For
		example, if you want to add a web address, please make sure that the website somehow references
		your Kusama/Polkadot address.</p>
	</div>
`;

const ContentEmailSecondChallenge = `
	<div class="col-10">
		<h2>⚠️️ Additional Challenge</h2>
		<p>An message containing an additional challenge was sent to <strong
			id="email-second-challenge"></strong>. Please insert that challenge in the following field:
		</p>
		<div class="input-group">
		<input id="specify-second-challenge" type="text" class="form-control"
			aria-label="Second challenge verification" placeholder="Challenge...">
		<button id="execute-second-challenge" class="col-1 btn btn-primary"
			type="button">Verify</button>
		</div>
	</div>
`;

const ContentVerificationOverview = `
	<div class="col-10 table-responsive ">
		<h2>Account verification</h2>
		<p>The service expects a message <strong>from</strong> the specified account in the on-chain
		identity sent <strong>to</strong> the corresponding W3F account containing the
		<strong>challenge</strong>. Do note that each account type has its own, individual
		challenge.
		</p>
		<table id="verification-overview" class="table table-striped table-dark"></table>
	</div>
`;