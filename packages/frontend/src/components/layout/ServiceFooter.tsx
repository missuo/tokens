"use client";

import styled from "styled-components";

export function ServiceFooter() {
  return (
    <Footer aria-label="About tokens.ci">
      <Inner>
        <Statement>
          <ProjectLink
            href="https://github.com/missuo/tokens"
            target="_blank"
            rel="noopener noreferrer"
          >
            tokens.ci
          </ProjectLink>{" "}
          is an open-source project built on top of{" "}
          <ProjectLink
            href="https://github.com/junhoyeo/tokscale"
            target="_blank"
            rel="noopener noreferrer"
          >
            Tokscale
          </ProjectLink>
          .
        </Statement>
      </Inner>
    </Footer>
  );
}

const Footer = styled.footer`
  width: 100%;
`;

const Inner = styled.div`
  width: 100%;
  max-width: 1200px;
  margin: 0 auto;
  padding: 10px 24px 24px;
  text-align: center;

  @media (max-width: 520px) {
    padding: 8px 16px 20px;
  }
`;

const Statement = styled.p`
  margin: 0;
  color: var(--service-text-muted);
  font-size: 0.75rem;
  line-height: 1.6;
`;

const ProjectLink = styled.a`
  color: inherit;
  font-weight: 500;
  text-decoration: underline;
  text-decoration-color: var(--service-border-strong);
  text-underline-offset: 3px;

  &:hover {
    color: var(--service-text);
    text-decoration-color: currentColor;
  }

  &:focus-visible {
    border-radius: 4px;
    outline: 2px solid var(--service-focus);
    outline-offset: 3px;
  }
`;
